//! Abertura de contêineres comprimidos (RF-104).
//!
//! Módulos circulam há décadas dentro de `.zip`, `.gz`, `.xz`, `.bz2` e do empacotador
//! PowerPacker do Amiga. O formato é reconhecido **pelo conteúdo**, nunca pela extensão, e o
//! resultado é desempacotado de novo até sobrar um módulo — arquivos aninhados existem.

use std::io::Read;

use thiserror::Error;

/// Quantas camadas de contêiner são abertas antes de desistir.
///
/// Contêiner que se contém é ataque, não descuido: sem este teto, um arquivo de poucos KB
/// derruba o processo.
const MAX_DEPTH: usize = 4;

/// Assinaturas reconhecidas, cada uma no início do arquivo.
const MAGIC_ZIP: &[u8] = b"PK\x03\x04";
const MAGIC_GZIP: &[u8] = &[0x1F, 0x8B];
const MAGIC_XZ: &[u8] = &[0xFD, b'7', b'z', b'X', b'Z', 0x00];
const MAGIC_BZIP2: &[u8] = b"BZh";
const MAGIC_POWERPACKER: &[u8] = b"PP20";

/// Falha ao abrir um contêiner.
#[derive(Debug, Error)]
pub enum ContainerError {
    #[error("dados comprimidos inválidos ({kind})")]
    Corrupt {
        kind: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("o arquivo {kind} está vazio")]
    Empty { kind: &'static str },

    #[error("o conteúdo descomprimido passa de {limit} bytes")]
    TooLarge { limit: u64 },

    #[error("aninhamento de contêineres passa de {MAX_DEPTH} camadas")]
    TooDeep,

    #[error("fluxo PowerPacker malformado: {0}")]
    PowerPacker(&'static str),
}

/// Abre os contêineres de `bytes` até sobrar o conteúdo final.
pub fn unpack(mut bytes: Vec<u8>, limit: u64) -> Result<Vec<u8>, ContainerError> {
    for _ in 0..MAX_DEPTH {
        let Some(kind) = detect(&bytes) else {
            return Ok(bytes);
        };
        bytes = open(kind, &bytes, limit)?;
    }

    if detect(&bytes).is_some() {
        return Err(ContainerError::TooDeep);
    }
    Ok(bytes)
}

/// Contêiner reconhecido pela assinatura no início do arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    Gzip,
    Xz,
    Bzip2,
    PowerPacker,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Gzip => "gzip",
            Self::Xz => "xz",
            Self::Bzip2 => "bzip2",
            Self::PowerPacker => "PowerPacker",
        }
    }
}

fn detect(bytes: &[u8]) -> Option<Kind> {
    let starts_with = |magic: &[u8]| bytes.starts_with(magic);
    if starts_with(MAGIC_ZIP) {
        return Some(Kind::Zip);
    }
    if starts_with(MAGIC_GZIP) {
        return Some(Kind::Gzip);
    }
    if starts_with(MAGIC_XZ) {
        return Some(Kind::Xz);
    }
    if starts_with(MAGIC_BZIP2) {
        return Some(Kind::Bzip2);
    }
    if starts_with(MAGIC_POWERPACKER) {
        return Some(Kind::PowerPacker);
    }
    None
}

fn open(kind: Kind, bytes: &[u8], limit: u64) -> Result<Vec<u8>, ContainerError> {
    match kind {
        Kind::Zip => open_zip(bytes, limit),
        Kind::Gzip => drain(flate2::read::GzDecoder::new(bytes), kind, limit),
        Kind::Bzip2 => drain(bzip2_rs::DecoderReader::new(bytes), kind, limit),
        Kind::Xz => open_xz(bytes, limit),
        Kind::PowerPacker => powerpacker::decompress(bytes, limit),
    }
}

/// Extrai a maior entrada do zip.
///
/// Coletâneas costumam trazer o módulo ao lado de `file_id.diz` e afins; a maior entrada é
/// quase sempre o módulo, e o custo de errar é um erro de formato logo adiante.
fn open_zip(bytes: &[u8], limit: u64) -> Result<Vec<u8>, ContainerError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|source| ContainerError::Corrupt {
        kind: Kind::Zip.label(),
        source: std::io::Error::other(source),
    })?;

    let mut largest: Option<(usize, u64)> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|source| ContainerError::Corrupt {
                kind: Kind::Zip.label(),
                source: std::io::Error::other(source),
            })?;
        if !entry.is_file() {
            continue;
        }
        if largest.is_none_or(|(_, size)| entry.size() > size) {
            largest = Some((index, entry.size()));
        }
    }

    let Some((index, _)) = largest else {
        return Err(ContainerError::Empty {
            kind: Kind::Zip.label(),
        });
    };
    let entry = archive
        .by_index(index)
        .map_err(|source| ContainerError::Corrupt {
            kind: Kind::Zip.label(),
            source: std::io::Error::other(source),
        })?;
    drain(entry, Kind::Zip, limit)
}

fn open_xz(bytes: &[u8], limit: u64) -> Result<Vec<u8>, ContainerError> {
    let mut output = Vec::new();
    let mut input = std::io::BufReader::new(bytes);
    lzma_rs::xz_decompress(&mut input, &mut output).map_err(|source| ContainerError::Corrupt {
        kind: Kind::Xz.label(),
        source: std::io::Error::other(source),
    })?;
    check_limit(output, limit)
}

fn drain(source: impl Read, kind: Kind, limit: u64) -> Result<Vec<u8>, ContainerError> {
    let mut output = Vec::new();
    // Um byte além do teto basta para detectar o estouro sem materializar a bomba inteira.
    source
        .take(limit.saturating_add(1))
        .read_to_end(&mut output)
        .map_err(|source| ContainerError::Corrupt {
            kind: kind.label(),
            source,
        })?;
    check_limit(output, limit)
}

fn check_limit(output: Vec<u8>, limit: u64) -> Result<Vec<u8>, ContainerError> {
    if output.len() as u64 > limit {
        return Err(ContainerError::TooLarge { limit });
    }
    Ok(output)
}

mod powerpacker {
    //! Descompressor do PowerPacker (`PP20`), o empacotador padrão do Amiga.
    //!
    //! O fluxo é lido **de trás para frente**: os últimos quatro bytes trazem o tamanho
    //! descomprimido (3 bytes) e quantos bits do fim descartar (1 byte), e o saída é
    //! preenchida do fim para o começo.
    //!
    //! Verificado apenas contra fluxos construídos à mão a partir da descrição do formato;
    //! ainda não foi confrontado com um arquivo `PP20` real (ver CLAUDE.md §12, item 1.3).

    use super::ContainerError;

    /// Cabeçalho: `PP20` seguido de quatro bytes de "eficiência" (nº de bits por offset).
    const HEADER_LEN: usize = 8;

    /// Rodapé: 3 bytes de tamanho descomprimido e 1 byte de bits a descartar.
    const FOOTER_LEN: usize = 4;

    /// Quantos grupos de contagem existem antes de o comprimento virar variável.
    const RUN_GROUPS: u32 = 3;

    pub fn decompress(bytes: &[u8], limit: u64) -> Result<Vec<u8>, ContainerError> {
        if bytes.len() < HEADER_LEN + FOOTER_LEN {
            return Err(ContainerError::PowerPacker("arquivo curto demais"));
        }

        let efficiency: Vec<u32> = bytes[4..HEADER_LEN]
            .iter()
            .map(|bits| u32::from(*bits))
            .collect();
        if efficiency.iter().any(|bits| *bits == 0 || *bits > 32) {
            return Err(ContainerError::PowerPacker("tabela de eficiência inválida"));
        }

        let footer = &bytes[bytes.len() - FOOTER_LEN..];
        let size =
            (usize::from(footer[0]) << 16) | (usize::from(footer[1]) << 8) | usize::from(footer[2]);
        if size as u64 > limit {
            return Err(ContainerError::TooLarge { limit });
        }

        let mut reader = BitReader::new(&bytes[HEADER_LEN..bytes.len() - FOOTER_LEN]);
        reader.skip(u32::from(footer[3]))?;

        let mut output = vec![0_u8; size];
        let mut cursor = size;

        while cursor > 0 {
            if reader.bit()? == 0 {
                copy_literals(&mut reader, &mut output, &mut cursor)?;
            }
            if cursor == 0 {
                break;
            }
            copy_match(&mut reader, &efficiency, &mut output, &mut cursor)?;
        }
        Ok(output)
    }

    fn copy_literals(
        reader: &mut BitReader<'_>,
        output: &mut [u8],
        cursor: &mut usize,
    ) -> Result<(), ContainerError> {
        let mut count = 1;
        loop {
            let extra = reader.bits(2)?;
            count += extra;
            if extra != 3 {
                break;
            }
        }
        for _ in 0..count {
            let byte = u8::try_from(reader.bits(8)?)
                .map_err(|_| ContainerError::PowerPacker("literal fora de faixa"))?;
            *cursor = cursor.checked_sub(1).ok_or(ContainerError::PowerPacker(
                "literais além do tamanho declarado",
            ))?;
            output[*cursor] = byte;
        }
        Ok(())
    }

    fn copy_match(
        reader: &mut BitReader<'_>,
        efficiency: &[u32],
        output: &mut [u8],
        cursor: &mut usize,
    ) -> Result<(), ContainerError> {
        let group = reader.bits(2)?;
        let bits = efficiency
            .get(group as usize)
            .copied()
            .ok_or(ContainerError::PowerPacker("grupo de offset inválido"))?;

        let (length, offset) = if group == RUN_GROUPS {
            // No último grupo o comprimento é variável e o offset pode usar menos bits.
            let wide = reader.bit()? == 1;
            let offset = reader.bits(if wide { bits } else { 7 })?;
            let mut length = RUN_GROUPS + 2;
            loop {
                let extra = reader.bits(3)?;
                length += extra;
                if extra != 7 {
                    break;
                }
            }
            (length, offset)
        } else {
            (group + 2, reader.bits(bits)?)
        };

        let source = *cursor + offset as usize + 1;
        if source > output.len() {
            return Err(ContainerError::PowerPacker("offset aponta fora da saída"));
        }
        for step in 0..length as usize {
            *cursor = cursor.checked_sub(1).ok_or(ContainerError::PowerPacker(
                "cópia além do tamanho declarado",
            ))?;
            output[*cursor] = output[source - 1 - step];
        }
        Ok(())
    }

    /// Leitor de bits que percorre o fluxo do fim para o começo, como o PowerPacker grava.
    struct BitReader<'a> {
        bytes: &'a [u8],
        /// Posição, em bits, contada a partir do fim do fluxo.
        position: usize,
    }

    impl<'a> BitReader<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self { bytes, position: 0 }
        }

        fn skip(&mut self, count: u32) -> Result<(), ContainerError> {
            for _ in 0..count {
                self.bit()?;
            }
            Ok(())
        }

        fn bit(&mut self) -> Result<u32, ContainerError> {
            let total = self.bytes.len() * u8::BITS as usize;
            if self.position >= total {
                return Err(ContainerError::PowerPacker("fluxo terminou antes da saída"));
            }
            let byte = self.bytes[self.bytes.len() - 1 - self.position / u8::BITS as usize];
            let bit = (byte >> (self.position % u8::BITS as usize)) & 1;
            self.position += 1;
            Ok(u32::from(bit))
        }

        fn bits(&mut self, count: u32) -> Result<u32, ContainerError> {
            let mut value = 0;
            for _ in 0..count {
                value = (value << 1) | self.bit()?;
            }
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn gzipped(payload: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(payload)
            .expect("escrita em memória não falha");
        encoder.finish().expect("finalização não falha")
    }

    #[test]
    fn conteudo_sem_conteiner_passa_intacto() {
        let bytes = b"IMPM nao e conteiner".to_vec();
        assert_eq!(unpack(bytes.clone(), 1024).expect("sem contêiner"), bytes);
    }

    #[test]
    fn gzip_e_aberto() {
        let packed = gzipped(b"conteudo do modulo");
        assert_eq!(
            unpack(packed, 1024).expect("gzip válido"),
            b"conteudo do modulo"
        );
    }

    #[test]
    fn aninhamento_e_aberto_ate_o_limite() {
        let packed = gzipped(&gzipped(b"dois niveis"));
        assert_eq!(unpack(packed, 1024).expect("dois níveis"), b"dois niveis");
    }

    #[test]
    fn aninhamento_excessivo_e_recusado() {
        let mut packed = b"fundo".to_vec();
        for _ in 0..MAX_DEPTH + 1 {
            packed = gzipped(&packed);
        }
        let error = unpack(packed, 4096).expect_err("deve recusar");
        assert!(matches!(error, ContainerError::TooDeep), "obtido: {error}");
    }

    #[test]
    fn bomba_de_descompressao_e_recusada() {
        let packed = gzipped(&vec![0_u8; 10_000]);
        let error = unpack(packed, 128).expect_err("deve recusar");
        assert!(
            matches!(error, ContainerError::TooLarge { limit: 128 }),
            "obtido: {error}"
        );
    }

    #[test]
    fn gzip_corrompido_vira_erro_e_nao_panico() {
        let mut packed = gzipped(b"conteudo");
        let last = packed.len() - 1;
        packed[last] ^= 0xFF;
        packed.truncate(packed.len() - 2);
        assert!(unpack(packed, 1024).is_err());
    }
}

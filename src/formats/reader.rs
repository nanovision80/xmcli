//! Acesso com verificação de limites a um bloco de bytes de entrada.
//!
//! Todo loader passa por aqui. O arquivo é hostil por definição (CLAUDE.md §2, invariante 6):
//! nenhum tamanho, offset ou contagem do cabeçalho é digno de confiança, e cada leitura diz
//! exatamente onde faltou byte quando falha (RF-102).

use thiserror::Error;

/// Leitura que caiu fora do arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{what}: esperava {need} byte(s) em 0x{offset:X}, o arquivo tem {len}")]
pub struct Truncated {
    /// O que se tentava ler, para a mensagem fazer sentido sem depurador.
    pub what: &'static str,
    pub offset: usize,
    pub need: usize,
    pub len: usize,
}

/// Bloco de bytes com acesso por deslocamento absoluto.
#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Verdadeiro quando a assinatura está presente no deslocamento dado.
    pub fn has_magic(&self, offset: usize, magic: &[u8]) -> bool {
        self.slice(offset, magic.len(), "assinatura")
            .is_ok_and(|found| found == magic)
    }

    pub fn slice(
        &self,
        offset: usize,
        len: usize,
        what: &'static str,
    ) -> Result<&'a [u8], Truncated> {
        let end = offset.checked_add(len).ok_or(Truncated {
            what,
            offset,
            need: len,
            len: self.bytes.len(),
        })?;
        self.bytes.get(offset..end).ok_or(Truncated {
            what,
            offset,
            need: len,
            len: self.bytes.len(),
        })
    }

    pub fn u8(&self, offset: usize, what: &'static str) -> Result<u8, Truncated> {
        Ok(self.slice(offset, 1, what)?[0])
    }

    pub fn u16_le(&self, offset: usize, what: &'static str) -> Result<u16, Truncated> {
        let bytes = self.slice(offset, 2, what)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn u16_be(&self, offset: usize, what: &'static str) -> Result<u16, Truncated> {
        let bytes = self.slice(offset, 2, what)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub fn u32_le(&self, offset: usize, what: &'static str) -> Result<u32, Truncated> {
        let bytes = self.slice(offset, 4, what)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leitura_alem_do_fim_diz_onde_faltou() {
        let reader = Reader::new(b"IMPM");
        let error = reader.u32_le(2, "campo").expect_err("passa do fim");
        assert_eq!(error.offset, 2);
        assert_eq!(error.need, 4);
        assert_eq!(error.len, 4);
    }

    #[test]
    fn deslocamento_absurdo_nao_estoura_a_soma() {
        let reader = Reader::new(b"IMPM");
        assert!(reader.slice(usize::MAX, 8, "campo").is_err());
    }
}

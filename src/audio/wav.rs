//! Escrita de WAV PCM de 16 bits (RF-403).
//!
//! São dois cabeçalhos de tamanho fixo e uma conversão de amostra. Uma dependência para isso
//! custaria mais em superfície de manutenção do que as quarenta linhas abaixo (CLAUDE.md §9).

use std::io::{Seek, SeekFrom, Write};

use crate::player::CHANNELS;

/// Código de formato do PCM inteiro no cabeçalho RIFF.
const FORMAT_PCM: u16 = 1;
/// Profundidade de bits da saída.
const BITS_PER_SAMPLE: u16 = 16;
/// Bytes que o campo de tamanho do RIFF não conta (os quatro primeiros e o próprio campo).
const RIFF_PREAMBLE: u32 = 8;
/// Tamanho do bloco `fmt ` para PCM inteiro.
const FMT_CHUNK_SIZE: u32 = 16;
/// Posição do campo de tamanho do RIFF, para o remendo final.
const RIFF_SIZE_OFFSET: u64 = 4;
/// Posição do campo de tamanho do bloco `data`.
const DATA_SIZE_OFFSET: u64 = 40;

/// Escreve o cabeçalho com tamanhos zerados; eles são corrigidos por [`finish`].
pub fn write_header(out: &mut impl Write, sample_rate: u32) -> std::io::Result<()> {
    let channels = CHANNELS as u16;
    let block_align = channels * BITS_PER_SAMPLE / 8;
    let byte_rate = sample_rate * u32::from(block_align);

    out.write_all(b"RIFF")?;
    out.write_all(&0_u32.to_le_bytes())?;
    out.write_all(b"WAVE")?;
    out.write_all(b"fmt ")?;
    out.write_all(&FMT_CHUNK_SIZE.to_le_bytes())?;
    out.write_all(&FORMAT_PCM.to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&byte_rate.to_le_bytes())?;
    out.write_all(&block_align.to_le_bytes())?;
    out.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&0_u32.to_le_bytes())
}

/// Converte quadros em ponto flutuante para PCM de 16 bits e escreve.
pub fn write_frames(out: &mut impl Write, samples: &[f32]) -> std::io::Result<()> {
    let mut encoded = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        encoded.extend_from_slice(&to_pcm16(*sample).to_le_bytes());
    }
    out.write_all(&encoded)
}

/// Volta aos campos de tamanho e grava os valores reais.
pub fn finish(out: &mut (impl Write + Seek), data_bytes: u32) -> std::io::Result<()> {
    out.seek(SeekFrom::Start(RIFF_SIZE_OFFSET))?;
    out.write_all(&(data_bytes + DATA_SIZE_OFFSET as u32 - RIFF_PREAMBLE).to_le_bytes())?;
    out.seek(SeekFrom::Start(DATA_SIZE_OFFSET))?;
    out.write_all(&data_bytes.to_le_bytes())?;
    out.flush()
}

/// Converte uma amostra normalizada em inteiro de 16 bits, cortando no limiar.
///
/// O corte é assimétrico de propósito: o inteiro de 16 bits vai de -32768 a 32767, e usar o
/// mesmo fator para os dois lados introduziria distorção no pico negativo.
fn to_pcm16(sample: f32) -> i16 {
    const PEAK: f32 = i16::MAX as f32;
    let scaled = sample.clamp(-1.0, 1.0) * PEAK;
    scaled.round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabecalho_tem_o_tamanho_canonico() {
        let mut out = Vec::new();
        write_header(&mut out, 44_100).expect("escrita em memória");
        assert_eq!(out.len(), DATA_SIZE_OFFSET as usize + 4);
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[36..40], b"data");
    }

    #[test]
    fn amostras_sao_cortadas_no_limiar() {
        assert_eq!(to_pcm16(0.0), 0);
        assert_eq!(to_pcm16(1.0), i16::MAX);
        assert_eq!(to_pcm16(-1.0), -i16::MAX);
        assert_eq!(to_pcm16(9.0), i16::MAX);
        assert_eq!(to_pcm16(f32::NAN), 0);
    }
}

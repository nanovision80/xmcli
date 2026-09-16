//! Renderização mais rápida que tempo real (RF-403, RF-404, RF-405).
//!
//! Sem dispositivo no caminho, este módulo também é o backend nulo: renderizar para um
//! destino que descarta os bytes dá medição determinística de custo de CPU.

use std::io::{Seek, Write};

use crate::audio::wav;
use crate::player::{CHANNELS, Engine};

/// Quadros por chamada ao motor. Grande o bastante para o custo por chamada sumir, pequeno o
/// bastante para o buffer caber no cache.
const BLOCK_FRAMES: usize = 4_096;

/// Formato dos bytes emitidos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// WAV PCM de 16 bits, com cabeçalho remendado no fim.
    Wav,
    /// Ponto flutuante de 32 bits, little-endian, intercalado — o mix interno, sem perda.
    RawFloat32,
}

/// Resultado de uma renderização.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rendered {
    pub frames: u64,
    pub sample_rate: u32,
}

impl Rendered {
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate)
    }
}

/// Renderiza a música inteira em um destino que permite voltar e remendar o cabeçalho.
pub fn render_wav(
    engine: &mut dyn Engine,
    sample_rate: u32,
    out: &mut (impl Write + Seek),
) -> std::io::Result<Rendered> {
    wav::write_header(out, sample_rate)?;
    let frames = pump(engine, |samples| wav::write_frames(out, samples))?;
    let data_bytes = u32::try_from(frames * CHANNELS as u64 * 2).unwrap_or(u32::MAX);
    wav::finish(out, data_bytes)?;
    Ok(Rendered {
        frames,
        sample_rate,
    })
}

/// Renderiza a música inteira em um destino sequencial, sem cabeçalho.
pub fn render_raw(
    engine: &mut dyn Engine,
    sample_rate: u32,
    out: &mut impl Write,
) -> std::io::Result<Rendered> {
    let frames = pump(engine, |samples| {
        let mut encoded = Vec::with_capacity(samples.len() * 4);
        for sample in samples {
            encoded.extend_from_slice(&sample.to_le_bytes());
        }
        out.write_all(&encoded)
    })?;
    out.flush()?;
    Ok(Rendered {
        frames,
        sample_rate,
    })
}

/// Puxa blocos do motor até o fim da música, entregando cada um ao destino.
fn pump(
    engine: &mut dyn Engine,
    mut sink: impl FnMut(&[f32]) -> std::io::Result<()>,
) -> std::io::Result<u64> {
    let mut buffer = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
    let mut total = 0_u64;
    loop {
        let frames = engine.render(&mut buffer);
        if frames == 0 {
            return Ok(total);
        }
        sink(&buffer[..frames * CHANNELS])?;
        total += frames as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::test_tone::TestTone;

    const RATE: u32 = 44_100;

    #[test]
    fn wav_tem_o_numero_de_quadros_pedido() {
        let mut engine = TestTone::new(RATE, 0.25);
        let mut out = std::io::Cursor::new(Vec::new());
        let rendered = render_wav(&mut engine, RATE, &mut out).expect("render em memória");

        assert_eq!(rendered.frames, u64::from(RATE) / 4);
        let bytes = out.into_inner();
        let declared = u32::from_le_bytes(bytes[40..44].try_into().expect("campo de 4 bytes"));
        assert_eq!(u64::from(declared), rendered.frames * CHANNELS as u64 * 2);
        assert_eq!(bytes.len() as u64, 44 + u64::from(declared));
    }

    #[test]
    fn raw_emite_quatro_bytes_por_amostra() {
        let mut engine = TestTone::new(RATE, 0.1);
        let mut out = Vec::new();
        let rendered = render_raw(&mut engine, RATE, &mut out).expect("render em memória");
        assert_eq!(out.len() as u64, rendered.frames * CHANNELS as u64 * 4);
    }

    #[test]
    fn musica_vazia_produz_wav_valido_e_vazio() {
        let mut engine = TestTone::new(RATE, 0.0);
        let mut out = std::io::Cursor::new(Vec::new());
        let rendered = render_wav(&mut engine, RATE, &mut out).expect("render em memória");
        assert_eq!(rendered.frames, 0);
        assert_eq!(out.into_inner().len(), 44);
    }
}

//! O motor de reprodução: transforma um módulo em amostras.
//!
//! Nesta etapa há uma implementação só, apoiada no libopenmpt, que dá som correto desde o
//! primeiro dia. A etapa 8 do roteiro traz o motor próprio — e é por isso que o resto do
//! programa fala com um [`Engine`], nunca com o libopenmpt diretamente.

#[cfg(feature = "engine-openmpt")]
mod openmpt;

#[cfg(feature = "engine-openmpt")]
pub use openmpt::OpenMptEngine;

use thiserror::Error;

use crate::bus::snapshot::Snapshot;

/// Canais na saída. Todo o caminho de áudio é estéreo intercalado.
pub const CHANNELS: usize = 2;

/// Falha ao preparar o motor de reprodução.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("o motor não conseguiu carregar o módulo")]
    Load,

    #[error("esta build não tem motor de reprodução; recompile com a feature engine-openmpt")]
    Unavailable,
}

/// Fonte de amostras de uma música.
///
/// O contrato é o de um gerador: cada chamada preenche o buffer com quadros estéreo
/// intercalados e devolve quantos **quadros** escreveu. Zero significa fim da música.
///
/// O motor **não** precisa ser `Send`: quem renderiza é a linha que chamou, e só o lado
/// consumidor do anel atravessa para o callback do dispositivo. Exigir `Send` obrigaria a
/// envolver o ponteiro do libopenmpt em `unsafe`, sem ganho nenhum.
pub trait Engine {
    fn render(&mut self, buffer: &mut [f32]) -> usize;

    /// Duração estimada da música, em segundos.
    fn duration_seconds(&mut self) -> f64;

    /// Salta para `seconds` do começo da música e devolve onde de fato parou (RF-207).
    ///
    /// O estado dos canais no destino — notas soando, efeitos em curso, andamento — tem de ser
    /// o que seria se a música tivesse tocado até ali: o seek reconstrói esse estado, não
    /// apenas pula para o padrão.
    fn seek(&mut self, seconds: f64) -> f64;

    /// Onde a música está agora, para o barramento de visualização (RF-521).
    ///
    /// Vale para o instante do último quadro renderizado: quem chama é o laço de render, e é
    /// ele que sabe em que quadro esse instante cai.
    fn snapshot(&mut self) -> Snapshot;
}

/// Constrói o motor desta build para o módulo dado.
///
/// A escolha do motor é de compilação enquanto existe um só. Quando a etapa 8 trouxer o motor
/// próprio, ela vira de execução, pelo `--engine`.
pub fn load(bytes: &[u8], sample_rate: u32) -> Result<Box<dyn Engine>, EngineError> {
    #[cfg(feature = "engine-openmpt")]
    {
        Ok(Box::new(OpenMptEngine::load(bytes, sample_rate)?))
    }
    #[cfg(not(feature = "engine-openmpt"))]
    {
        let _ = (bytes, sample_rate);
        Err(EngineError::Unavailable)
    }
}

#[cfg(test)]
pub mod test_tone {
    //! Motor de teste: uma senoide de duração fixa.
    //!
    //! Existe para exercitar o caminho de renderização e de escrita sem depender do
    //! libopenmpt — é o segundo implementador que justifica o trait [`Engine`].

    use super::{CHANNELS, Engine};
    use crate::bus::snapshot::Snapshot;

    /// Frequência da senoide, em Hz.
    const FREQUENCY_HZ: f32 = 440.0;
    /// Amplitude, com folga para o limiar do formato de 16 bits.
    const AMPLITUDE: f32 = 0.5;

    pub struct TestTone {
        sample_rate: u32,
        total_frames: usize,
        remaining_frames: usize,
        phase: f32,
    }

    impl TestTone {
        pub fn new(sample_rate: u32, seconds: f64) -> Self {
            let total_frames = (f64::from(sample_rate) * seconds) as usize;
            Self {
                sample_rate,
                total_frames,
                remaining_frames: total_frames,
                phase: 0.0,
            }
        }
    }

    impl Engine for TestTone {
        fn render(&mut self, buffer: &mut [f32]) -> usize {
            let frames = (buffer.len() / CHANNELS).min(self.remaining_frames);
            let step = std::f32::consts::TAU * FREQUENCY_HZ / self.sample_rate as f32;
            for frame in 0..frames {
                let value = self.phase.sin() * AMPLITUDE;
                buffer[frame * CHANNELS] = value;
                buffer[frame * CHANNELS + 1] = value;
                self.phase = (self.phase + step) % std::f32::consts::TAU;
            }
            self.remaining_frames -= frames;
            frames
        }

        fn duration_seconds(&mut self) -> f64 {
            self.remaining_frames as f64 / f64::from(self.sample_rate)
        }

        /// Uma senoide não tem estado além da fase, e a fase no destino não importa ao teste.
        fn seek(&mut self, seconds: f64) -> f64 {
            let target =
                ((f64::from(self.sample_rate) * seconds.max(0.0)) as usize).min(self.total_frames);
            self.remaining_frames = self.total_frames - target;
            target as f64 / f64::from(self.sample_rate)
        }

        /// Uma senoide não tem padrão, linha nem canal de tracker: o estado é o ocioso.
        fn snapshot(&mut self) -> Snapshot {
            Snapshot::IDLE
        }
    }
}

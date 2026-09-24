//! Motor apoiado no libopenmpt (fase 1 do roteiro, CLAUDE.md §6).
//!
//! O libopenmpt é a referência de precisão para os quatro formatos: usá-lo desde o começo
//! tira o risco de reprodução errada do caminho crítico enquanto a interface é construída.
//! Em troca, o binário passa a exigir a biblioteca no sistema — dívida que a etapa 8 quita.

use openmpt::module::{Logger, Module};

use super::{CHANNELS, Engine, EngineError};
use crate::bus::snapshot::{MAX_CHANNELS, Snapshot, Vu};

pub struct OpenMptEngine {
    module: Module,
    sample_rate: u32,
    /// O wrapper do libopenmpt recebe `&mut Vec`, não fatia; este é o buffer reaproveitado
    /// entre chamadas para a conversão não alocar a cada bloco.
    scratch: Vec<f32>,
}

impl OpenMptEngine {
    /// Carrega um módulo já desempacotado na memória.
    pub fn load(bytes: &[u8], sample_rate: u32) -> Result<Self, EngineError> {
        // O wrapper pede um Vec mutável porque a API C recebe ponteiro não-const.
        let mut owned = bytes.to_vec();
        let module = Module::create_from_memory(&mut owned, Logger::None, &[])
            .map_err(|()| EngineError::Load)?;
        Ok(Self {
            module,
            sample_rate,
            scratch: Vec::new(),
        })
    }
}

impl Engine for OpenMptEngine {
    fn render(&mut self, buffer: &mut [f32]) -> usize {
        // O libopenmpt escreve quadros intercalados e devolve a contagem em quadros; ele
        // preenche o que couber, então basta repassar o buffer inteiro.
        debug_assert!(
            buffer.len() >= CHANNELS,
            "buffer menor que um quadro estéreo"
        );
        if self.scratch.len() != buffer.len() {
            self.scratch.resize(buffer.len(), 0.0);
        }
        let frames = self
            .module
            .read_interleaved_float_stereo(self.sample_rate as i32, &mut self.scratch);
        let written = frames * CHANNELS;
        buffer[..written].copy_from_slice(&self.scratch[..written]);
        frames
    }

    fn duration_seconds(&mut self) -> f64 {
        self.module.get_duration_seconds()
    }

    fn seek(&mut self, seconds: f64) -> f64 {
        // O libopenmpt reconstrói o estado dos canais simulando a música até o destino, sem
        // mixar: é o fast-forward silencioso do RF-207, feito por ele. O motor próprio da
        // etapa 8 terá de fazer o mesmo.
        self.module.set_position_seconds(seconds)
    }

    fn snapshot(&mut self) -> Snapshot {
        // Canais além do teto de padrão dos quatro formatos só aparecem em módulos de
        // extensões que este programa não abre; ignorá-los é preferível a truncar o arranjo
        // em silêncio dentro do laço.
        let channels = clamp_to_u16(self.module.get_num_channels()).min(MAX_CHANNELS as u16);
        let mut snapshot = Snapshot {
            order: clamp_to_u16(self.module.get_current_order()),
            pattern: clamp_to_u16(self.module.get_current_pattern()),
            row: clamp_to_u16(self.module.get_current_row()),
            speed: clamp_to_u16(self.module.get_current_speed()).min(u16::from(u8::MAX)) as u8,
            bpm: clamp_to_u16(self.module.get_current_tempo()),
            voices: clamp_to_u16(self.module.get_current_playing_channels()),
            channels,
            ..Snapshot::IDLE
        };

        for index in 0..channels {
            snapshot.vu[usize::from(index)] = Vu {
                left: self.module.get_current_channel_vu_left(i32::from(index)),
                right: self.module.get_current_channel_vu_right(i32::from(index)),
            };
        }
        snapshot
    }
}

/// Converte uma contagem do libopenmpt, que usa `int` e responde negativo quando não sabe.
///
/// Antes do primeiro tick, e depois do fim da música, posição e tempo não existem: o valor
/// negativo vira zero, que é o mesmo que o estado ocioso mostra.
fn clamp_to_u16(value: i32) -> u16 {
    u16::try_from(value.max(0)).unwrap_or(u16::MAX)
}

//! Motor apoiado no libopenmpt (fase 1 do roteiro, CLAUDE.md §6).
//!
//! O libopenmpt é a referência de precisão para os quatro formatos: usá-lo desde o começo
//! tira o risco de reprodução errada do caminho crítico enquanto a interface é construída.
//! Em troca, o binário passa a exigir a biblioteca no sistema — dívida que a etapa 8 quita.

use openmpt::module::{Logger, Module};

use super::{CHANNELS, Engine, EngineError};

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
}

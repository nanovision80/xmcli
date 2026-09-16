//! Relógio de reprodução (RF-406).
//!
//! A conta que importa não é quantas amostras foram mixadas, e sim quantas o dispositivo já
//! **consumiu**: entre uma coisa e outra existe o buffer, e é essa diferença que faz a imagem
//! adiantar em relação ao som. A compensação propriamente dita entra na etapa 3 (RF-620);
//! aqui fica a contagem que ela vai usar.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Contagem compartilhada entre a linha de áudio e o resto do programa.
///
/// Escrita só pelo callback do dispositivo, lida por todo mundo. `Relaxed` basta: ninguém
/// depende de ordem com outra memória, só do valor mais recente que der.
#[derive(Debug, Default)]
pub struct Clock {
    frames_played: AtomicU64,
    underruns: AtomicU64,
    finished: AtomicBool,
}

impl Clock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Registra quadros entregues ao dispositivo. Chamado de dentro do callback.
    pub fn advance(&self, frames: u64) {
        self.frames_played.fetch_add(frames, Ordering::Relaxed);
    }

    /// Registra que o callback pediu mais do que havia pronto (RF-402).
    pub fn record_underrun(&self, frames: u64) {
        self.underruns.fetch_add(frames, Ordering::Relaxed);
    }

    /// Marca que o motor chegou ao fim da música e não produzirá mais nada.
    pub fn mark_finished(&self) {
        self.finished.store(true, Ordering::Release);
    }

    pub fn frames_played(&self) -> u64 {
        self.frames_played.load(Ordering::Relaxed)
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contagem_acumula_e_o_fim_e_visivel() {
        let clock = Clock::new();
        clock.advance(128);
        clock.advance(128);
        clock.record_underrun(64);
        assert_eq!(clock.frames_played(), 256);
        assert_eq!(clock.underruns(), 64);

        assert!(!clock.is_finished());
        clock.mark_finished();
        assert!(clock.is_finished());
    }
}

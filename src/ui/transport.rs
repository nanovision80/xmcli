//! O que cada tecla do transporte faz, dado o estado do player (RF-503).
//!
//! As regras são as do Winamp: `play` com a música tocando recomeça a faixa, `pause` alterna
//! entre tocando e pausado mas não tira do parado, e parar volta ao começo da faixa.

use crate::config::Action;
use crate::ui::Exit;

/// Estado do player, que o botão ativo do transporte mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Playing,
    Paused,
    /// No começo da faixa, sem tocar.
    Stopped,
}

/// O que fazer depois de uma tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nada muda.
    Stay,
    /// Pausar esta reprodução.
    Pause,
    /// Retomar esta reprodução, pausada ou parada no começo.
    Resume,
    /// Levar a música um passo para a frente (`true`) ou para trás (`false`).
    Seek { forward: bool },
    /// Esta reprodução termina.
    Leave(Exit),
}

pub fn press(state: Transport, action: Action) -> Step {
    use Transport::{Paused, Playing, Stopped};

    match (action, state) {
        (Action::Quit, _) => Step::Leave(Exit::Quit),
        (Action::Previous, _) => Step::Leave(Exit::Previous),
        (Action::Next, _) => Step::Leave(Exit::Next),
        // Parar é voltar ao começo: a faixa é aberta de novo, parada.
        (Action::Stop, Stopped) => Step::Stay,
        (Action::Stop, Playing | Paused) => Step::Leave(Exit::Rewind(Stopped)),
        (Action::Play, Playing) => Step::Leave(Exit::Rewind(Playing)),
        (Action::Play, Paused | Stopped) => Step::Resume,
        (Action::Pause, Playing) => Step::Pause,
        (Action::Pause, Paused) => Step::Resume,
        (Action::Pause, Stopped) => Step::Stay,
        (Action::PlayPause, Playing) => Step::Pause,
        (Action::PlayPause, Paused | Stopped) => Step::Resume,
        // Parado é o começo da faixa esperando o play, como no Winamp: as setas não o movem.
        (Action::SeekForward | Action::SeekBackward, Stopped) => Step::Stay,
        (Action::SeekForward, Playing | Paused) => Step::Seek { forward: true },
        (Action::SeekBackward, Playing | Paused) => Step::Seek { forward: false },
    }
}

/// Para onde um passo de seek leva a música, sem sair da faixa.
///
/// Passar do fim é chegar ao fim, e a faixa termina; antes do começo é o começo.
pub fn seek_target(position: f64, duration: f64, step: f64, forward: bool) -> f64 {
    let target = if forward {
        position + step
    } else {
        position - step
    };
    target.clamp(0.0, duration.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Transport::{Paused, Playing, Stopped};

    #[test]
    fn play_e_pause_alternam_sem_reabrir_a_faixa() {
        assert_eq!(press(Playing, Action::PlayPause), Step::Pause);
        assert_eq!(press(Paused, Action::PlayPause), Step::Resume);
        assert_eq!(press(Stopped, Action::PlayPause), Step::Resume);
        assert_eq!(press(Playing, Action::Pause), Step::Pause);
        assert_eq!(press(Paused, Action::Pause), Step::Resume);
        assert_eq!(press(Paused, Action::Play), Step::Resume);
        assert_eq!(press(Stopped, Action::Play), Step::Resume);
    }

    #[test]
    fn parado_so_sai_com_play() {
        assert_eq!(press(Stopped, Action::Pause), Step::Stay);
        assert_eq!(press(Stopped, Action::Stop), Step::Stay);
    }

    #[test]
    fn parar_e_play_tocando_voltam_ao_comeco() {
        assert_eq!(
            press(Playing, Action::Stop),
            Step::Leave(Exit::Rewind(Stopped))
        );
        assert_eq!(
            press(Paused, Action::Stop),
            Step::Leave(Exit::Rewind(Stopped))
        );
        assert_eq!(
            press(Playing, Action::Play),
            Step::Leave(Exit::Rewind(Playing))
        );
    }

    #[test]
    fn setas_movem_a_musica_so_fora_do_parado() {
        for state in [Playing, Paused] {
            assert_eq!(
                press(state, Action::SeekForward),
                Step::Seek { forward: true }
            );
            assert_eq!(
                press(state, Action::SeekBackward),
                Step::Seek { forward: false }
            );
        }
        assert_eq!(press(Stopped, Action::SeekForward), Step::Stay);
        assert_eq!(press(Stopped, Action::SeekBackward), Step::Stay);
    }

    #[test]
    fn o_seek_nao_sai_da_faixa() {
        assert_eq!(seek_target(10.0, 100.0, 5.0, true), 15.0);
        assert_eq!(seek_target(10.0, 100.0, 5.0, false), 5.0);
        assert_eq!(seek_target(2.0, 100.0, 5.0, false), 0.0);
        assert_eq!(seek_target(98.0, 100.0, 5.0, true), 100.0);
    }

    #[test]
    fn faixas_e_saida_valem_em_qualquer_estado() {
        for state in [Playing, Paused, Stopped] {
            assert_eq!(press(state, Action::Previous), Step::Leave(Exit::Previous));
            assert_eq!(press(state, Action::Next), Step::Leave(Exit::Next));
            assert_eq!(press(state, Action::Quit), Step::Leave(Exit::Quit));
        }
    }
}

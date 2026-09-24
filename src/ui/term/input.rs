//! O que chega do terminal: teclas e mudança de tamanho (RF-513, etapa 5.8).
//!
//! Tudo vem pelos eventos do `crossterm`: no Unix a mudança de tamanho vem do `SIGWINCH`, no
//! Windows dos eventos do console; as teclas, do mesmo lugar. Os eventos saem **um por vez**, e
//! quem junta é o laço de desenho. Juntar aqui obrigaria a esvaziar a fila, e esvaziar a fila
//! engoliria a tecla apertada no meio de um arrasto de janela.
//!
//! A mudança de tamanho não traz o tamanho. Arrastar a borda dispara dezenas de avisos, e o
//! número que vem no primeiro já está velho quando o laço chega a ele: o laço anota que mudou
//! e pergunta o tamanho ao terminal uma vez, na hora de desenhar o quadro seguinte.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::{Action, Key, KeyCode, Keymap};

/// Um evento do terminal que interessa à interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// O terminal mudou de tamanho.
    Resized,
    /// Uma tecla ligada a uma ação.
    Action(Action),
}

/// O próximo evento que interessa, esperando no máximo até `deadline`.
///
/// `None` quando o prazo passou sem nada. Tecla sem ação ligada não acorda ninguém.
pub fn next(deadline: Instant, keymap: &Keymap) -> io::Result<Option<Input>> {
    next_with(deadline, keymap, event::poll, event::read)
}

/// [`next`] com a fonte de eventos trocável, para os testes.
fn next_with(
    deadline: Instant,
    keymap: &Keymap,
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut read: impl FnMut() -> io::Result<Event>,
) -> io::Result<Option<Input>> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !poll(remaining)? {
            return Ok(None);
        }
        let input = match read()? {
            Event::Resize(..) => Some(Input::Resized),
            Event::Key(event) => key(event)
                .and_then(|key| keymap.action(key))
                .map(Input::Action),
            _ => None,
        };
        if input.is_some() {
            return Ok(input);
        }
    }
}

/// A tecla de um evento do `crossterm`, se for um aperto de uma tecla que o keymap conhece.
fn key(event: KeyEvent) -> Option<Key> {
    // O Windows avisa também quando a tecla sobe; uma ação por aperto, não duas.
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let code = match event.code {
        event::KeyCode::Char(ch) => KeyCode::Char(ch),
        event::KeyCode::Enter => KeyCode::Enter,
        event::KeyCode::Tab => KeyCode::Tab,
        event::KeyCode::Esc => KeyCode::Esc,
        event::KeyCode::Backspace => KeyCode::Backspace,
        event::KeyCode::Left => KeyCode::Left,
        event::KeyCode::Right => KeyCode::Right,
        event::KeyCode::Up => KeyCode::Up,
        event::KeyCode::Down => KeyCode::Down,
        event::KeyCode::Home => KeyCode::Home,
        event::KeyCode::End => KeyCode::End,
        event::KeyCode::PageUp => KeyCode::PageUp,
        event::KeyCode::PageDown => KeyCode::PageDown,
        event::KeyCode::F(number) => KeyCode::F(number),
        _ => return None,
    };
    Some(Key {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};

    use crossterm::event::{KeyEventState, KeyModifiers};

    use super::*;

    const LONG: Duration = Duration::from_secs(60);

    fn keymap() -> Keymap {
        let names = HashMap::from([("q".to_owned(), Action::Quit)]);
        Keymap::from_names(names).expect("atalho de teste é válido")
    }

    fn press(ch: char, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: event::KeyCode::Char(ch),
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }

    fn q() -> Event {
        press('q', KeyModifiers::NONE, KeyEventKind::Press)
    }

    /// Todos os eventos que interessam numa fila roteirizada, na ordem, até ela acabar.
    fn drain(events: Vec<Event>) -> Vec<Input> {
        let queue = RefCell::new(VecDeque::from(events));
        let keymap = keymap();
        let mut inputs = Vec::new();
        while let Some(input) = next_with(
            Instant::now() + LONG,
            &keymap,
            |_| Ok(!queue.borrow().is_empty()),
            || {
                queue
                    .borrow_mut()
                    .pop_front()
                    .ok_or_else(|| io::Error::other("leitura com a fila vazia"))
            },
        )
        .expect("a fila roteirizada não falha")
        {
            inputs.push(input);
        }
        inputs
    }

    #[test]
    fn prazo_sem_evento_devolve_nada() {
        assert_eq!(drain(vec![]), []);
    }

    #[test]
    fn tecla_no_meio_de_um_arrasto_nao_se_perde() {
        let events = vec![Event::Resize(90, 30), q(), Event::Resize(100, 35)];
        assert_eq!(
            drain(events),
            [Input::Resized, Input::Action(Action::Quit), Input::Resized]
        );
    }

    #[test]
    fn tecla_sem_acao_e_tecla_solta_nao_contam() {
        let events = vec![
            press('x', KeyModifiers::NONE, KeyEventKind::Press),
            press('q', KeyModifiers::NONE, KeyEventKind::Release),
            press('q', KeyModifiers::CONTROL, KeyEventKind::Press),
        ];
        assert_eq!(drain(events), []);
    }

    #[test]
    fn tecla_repetida_segurando_conta() {
        assert_eq!(
            drain(vec![press('q', KeyModifiers::NONE, KeyEventKind::Repeat)]),
            [Input::Action(Action::Quit)]
        );
    }

    #[test]
    fn shift_nao_atrapalha_a_letra() {
        // Alguns terminais mandam `Q` com SHIFT marcado; a letra já diz tudo.
        let names = HashMap::from([("Q".to_owned(), Action::Quit)]);
        let keymap = Keymap::from_names(names).expect("atalho de teste é válido");
        let event = KeyEvent {
            code: event::KeyCode::Char('Q'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        assert_eq!(
            key(event).and_then(|key| keymap.action(key)),
            Some(Action::Quit)
        );
    }
}

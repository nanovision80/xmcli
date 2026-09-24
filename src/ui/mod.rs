//! Interface de terminal: o player estilo Winamp (RF-500).
//!
//! [`term`] conversa com o terminal; [`chrome`] desenha o player sobre ele. O palco entra na
//! etapa 6. [`run`] é o laço que junta tudo enquanto a música toca.

pub mod chrome;
pub mod term;

use std::io;
use std::time::Instant;

use crate::audio::device::Playback;
use crate::config::{Action, Keymap, Settings, Theme};
use crate::ui::term::Session;
use crate::ui::term::buffer::Buffer;
use crate::ui::term::color::ColorDepth;
use crate::ui::term::input::{self, Input};
use crate::ui::term::pace::Pacer;
use crate::ui::term::paint::Painter;

/// Por que a interface terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// A música chegou ao fim.
    Ended,
    /// O usuário pediu para sair.
    Quit,
}

/// O que a interface precisa saber, resolvido uma vez antes do laço.
pub struct Setup<'a> {
    pub settings: &'a Settings,
    pub theme: &'a Theme,
    pub keymap: &'a Keymap,
    /// A profundidade de cor que o terminal aceita (RF-514).
    pub colors: ColorDepth,
}

/// Mostra o player até a música acabar ou o usuário sair.
///
/// Quadro a quadro: desenha, escreve de uma vez, e espera o intervalo que o [`Pacer`] manda
/// atendendo as teclas. A interface perde quadros, o áudio nunca (CLAUDE.md §2, invariante 2):
/// nada aqui espera pela linha de áudio nem a faz esperar.
pub fn run(playback: &mut Playback, setup: &Setup) -> io::Result<Exit> {
    let mut session = Session::enter()?;
    let mut painter = Painter::new(setup.colors);
    let mut pacer = Pacer::new(&setup.settings.ui, setup.colors);
    let (width, height) = session.size()?;
    let mut frame = Buffer::new(width, height);

    loop {
        if playback.is_finished() {
            return Ok(Exit::Ended);
        }
        chrome::frame::draw(&mut frame, setup.theme);

        let started = Instant::now();
        let bytes = painter.present(&frame, session.out())?;
        let pace = pacer.record(bytes, started.elapsed());
        painter.set_depth(pace.depth);

        // Uma rajada de avisos de tamanho vira uma pergunta só ao terminal, no quadro seguinte.
        let mut resized = false;
        while let Some(input) = input::next(started + pace.interval, setup.keymap)? {
            match input {
                Input::Resized => resized = true,
                Input::Action(Action::Quit) => {
                    playback.stop();
                    return Ok(Exit::Quit);
                }
            }
        }
        if resized {
            let (width, height) = session.size()?;
            frame = Buffer::new(width, height);
            painter.invalidate();
        }
    }
}

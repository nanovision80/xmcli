//! Interface de terminal: o player estilo Winamp (RF-500).
//!
//! [`term`] conversa com o terminal; [`chrome`] desenha o player sobre ele. O palco entra na
//! etapa 6. [`run`] é o laço que junta tudo enquanto a música toca.

pub mod chrome;
pub mod term;
pub mod transport;

use std::io;
use std::time::Instant;

use crate::audio::device::{Command, Playback};
use crate::config::{Action, Keymap, Settings, Theme, TimeDisplay};
use crate::ui::chrome::frame::View;
use crate::ui::term::Session;
use crate::ui::term::buffer::Buffer;
use crate::ui::term::color::ColorDepth;
use crate::ui::term::input::{self, Input};
use crate::ui::term::pace::Pacer;
use crate::ui::term::paint::Painter;
use crate::ui::transport::{Step, Transport};

/// Por que a interface terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// A música chegou ao fim.
    Ended,
    /// O usuário pediu para sair.
    Quit,
    /// Faixa anterior da lista.
    Previous,
    /// Próxima faixa da lista.
    Next,
    /// A mesma faixa, do começo, no estado dado: parada ou tocando.
    Rewind(Transport),
}

/// O que a interface precisa saber, resolvido uma vez antes do laço.
pub struct Setup<'a> {
    pub settings: &'a Settings,
    pub theme: &'a Theme,
    pub keymap: &'a Keymap,
    /// A profundidade de cor que o terminal aceita (RF-514).
    pub colors: ColorDepth,
}

/// O terminal nas mãos do player, aberto uma vez para a lista inteira.
///
/// Entrar e sair da tela alternativa a cada faixa mostraria o shell por um instante em cada
/// troca. O que se aprende sobre o terminal — o throughput do `Pacer`, o que já está na tela —
/// também vale de uma faixa para a outra.
pub struct Screen {
    session: Session,
    painter: Painter,
    pacer: Pacer,
    frame: Buffer,
    /// O modo do display de tempo que o usuário escolheu, que vale para a lista inteira.
    time: TimeDisplay,
}

impl Screen {
    pub fn open(setup: &Setup) -> io::Result<Self> {
        let session = Session::enter()?;
        let (width, height) = session.size()?;
        Ok(Self {
            session,
            painter: Painter::new(setup.colors),
            pacer: Pacer::new(&setup.settings.ui, setup.colors),
            frame: Buffer::new(width, height),
            time: setup.settings.ui.time_display,
        })
    }
}

/// Mostra o player até a música acabar ou o usuário deixar a faixa; `title` é o que o marquee
/// rola, e `state` é como a reprodução começou: tocando, ou parada no começo.
///
/// Quadro a quadro: desenha, escreve de uma vez, e espera o intervalo que o [`Pacer`] manda
/// atendendo as teclas. A interface perde quadros, o áudio nunca (CLAUDE.md §2, invariante 2):
/// nada aqui espera pela linha de áudio nem a faz esperar.
pub fn run(
    screen: &mut Screen,
    playback: &mut Playback,
    setup: &Setup,
    title: &str,
    mut state: Transport,
) -> io::Result<Exit> {
    let opened = Instant::now();

    loop {
        if playback.is_finished() {
            return Ok(Exit::Ended);
        }
        let view = View {
            title,
            marquee: &setup.settings.ui.marquee,
            elapsed: opened.elapsed(),
            transport: state,
            position: playback.clock.song_seconds(),
            duration: playback.duration_seconds,
            time: screen.time,
        };
        chrome::frame::draw(&mut screen.frame, setup.theme, &view);

        let started = Instant::now();
        let bytes = screen
            .painter
            .present(&screen.frame, screen.session.out())?;
        let pace = screen.pacer.record(bytes, started.elapsed());
        screen.painter.set_depth(pace.depth);

        // Uma rajada de avisos de tamanho vira uma pergunta só ao terminal, no quadro seguinte.
        let mut resized = false;
        while let Some(input) = input::next(started + pace.interval, setup.keymap)? {
            match input {
                Input::Resized => resized = true,
                Input::Action(Action::ToggleTime) => screen.time = screen.time.toggled(),
                Input::Action(action) => match transport::press(state, action) {
                    Step::Stay => {}
                    // Fila cheia: o pedido não entrou, e o botão não pode mentir.
                    Step::Pause if playback.send(Command::Pause) => state = Transport::Paused,
                    Step::Resume if playback.send(Command::Resume) => state = Transport::Playing,
                    Step::Pause | Step::Resume => {}
                    Step::Seek { forward } => {
                        let target = transport::seek_target(
                            playback.clock.song_seconds(),
                            playback.duration_seconds,
                            f64::from(setup.settings.ui.seek_step_seconds),
                            forward,
                        );
                        // Fila cheia com a tecla segurada: o próximo toque tenta de novo.
                        let _ = playback.send(Command::Seek(target));
                    }
                    Step::Leave(exit) => {
                        playback.stop();
                        return Ok(exit);
                    }
                },
            }
        }
        if resized {
            let (width, height) = screen.session.size()?;
            screen.frame = Buffer::new(width, height);
            screen.painter.invalidate();
        }
    }
}

//! Superfície de desenho: modo raw, tela alternativa e cursor oculto (RF-515), com o terminal
//! restaurado em qualquer saída (RF-516).
//!
//! Quem desfaz o que a [`Session`] fez é uma função só, [`restore`], e ela age **uma vez**. Três
//! caminhos chegam a ela, porque há três jeitos de o processo terminar:
//!
//! * a sessão sai de escopo — saída normal, ou pânico que desenrola a pilha;
//! * o hook de pânico — com `panic = "abort"`, que é o perfil de release, não há desenrolar e o
//!   `Drop` nunca roda. E mesmo desenrolando, o hook roda **antes** de imprimir a mensagem, que
//!   de outro jeito seria escrita na tela alternativa e sumiria com ela;
//! * a linha de sinais, no Unix — `SIGINT`, `SIGTERM` e `SIGHUP` terminam o processo sem
//!   desenrolar nada.
//!
//! Agir uma vez não é detalhe: a sequência que sai da tela alternativa também devolve o cursor à
//! posição salva na entrada. Repetida depois do hook de pânico, ela levaria o cursor de volta
//! para cima, e o prompt do shell escreveria por cima da mensagem.
//!
//! Quantas cores a superfície aceita é decidido à parte, em [`color`]. O quadro é desenhado
//! numa grade de células ([`buffer`]) e chega ao terminal por [`paint`], no ritmo que [`pace`]
//! decide. Teclas e mudança de tamanho chegam por [`input`].

pub mod buffer;
pub mod color;
pub mod input;
pub mod pace;
pub mod paint;

use std::io::{self, Stdout, Write};
use std::panic;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Há uma sessão com o terminal alterado, e ainda ninguém o restaurou.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// O hook de pânico e a linha de sinais, instalados na primeira sessão.
///
/// Guarda só o tipo do erro porque `io::Error` não é `Clone`, e a falha precisa ser devolvida a
/// cada sessão que tentar entrar.
static GUARDS: OnceLock<Result<(), io::ErrorKind>> = OnceLock::new();

/// O terminal inteiro nas mãos do player: modo raw, tela alternativa e cursor oculto.
///
/// Uma por processo — são estados do terminal, não do objeto. Uma segunda sessão ao mesmo tempo
/// é recusada.
#[derive(Debug)]
pub struct Session {
    out: Stdout,
}

impl Session {
    pub fn enter() -> io::Result<Self> {
        // Os guardas entram antes do primeiro byte: um sinal no meio da entrada também precisa
        // encontrar quem restaure.
        if let Err(kind) = GUARDS.get_or_init(|| install_guards().map_err(|error| error.kind())) {
            return Err(io::Error::new(
                *kind,
                "não foi possível instalar a restauração do terminal",
            ));
        }
        if ACTIVE.swap(true, Ordering::AcqRel) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "o terminal já está em uma sessão",
            ));
        }

        // O modo raw vem primeiro para que nenhuma tecla digitada durante a troca de tela
        // apareça em eco.
        if let Err(error) = enable_raw_mode() {
            ACTIVE.store(false, Ordering::Release);
            return Err(error);
        }
        // Daqui em diante o `Drop` restaura, inclusive se a troca de tela falhar.
        let mut session = Self { out: io::stdout() };
        enter_screen(&mut session.out)?;
        Ok(session)
    }

    /// Onde desenhar.
    pub fn out(&mut self) -> &mut Stdout {
        &mut self.out
    }

    /// Largura e altura do terminal, em células.
    pub fn size(&self) -> io::Result<(u16, u16)> {
        crossterm::terminal::size()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        restore();
    }
}

/// Devolve o terminal ao estado de antes da sessão, se ninguém o fez ainda.
fn restore() {
    if !ACTIVE.swap(false, Ordering::AcqRel) {
        return;
    }
    // Não há a quem informar uma falha: quem restaura pode ser um pânico ou um sinal, e o
    // terminal que não voltou não tem como mostrar mensagem nenhuma.
    let _ = leave_screen(&mut io::stdout());
    let _ = disable_raw_mode();
}

fn enter_screen(out: &mut impl Write) -> io::Result<()> {
    execute!(out, EnterAlternateScreen, Hide)
}

/// O inverso de [`enter_screen`], na ordem inversa.
fn leave_screen(out: &mut impl Write) -> io::Result<()> {
    execute!(out, Show, LeaveAlternateScreen)
}

fn install_guards() -> io::Result<()> {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));

    #[cfg(unix)]
    signals::install()?;
    Ok(())
}

/// Sinais que terminam o processo sem desenrolar a pilha.
///
/// No Windows não há equivalente a tratar: em modo raw o `Ctrl+C` chega como tecla, e fechar o
/// console encerra o console junto.
#[cfg(unix)]
mod signals {
    use std::io;

    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    use signal_hook::low_level::emulate_default_handler;

    /// Nome da linha que espera pelos sinais, para quem a vir num depurador.
    const THREAD_NAME: &str = "xmcli-sinais";

    /// `SIGINT` aqui vem de `kill -INT`: em modo raw o `Ctrl+C` não gera sinal, chega como
    /// tecla. `SIGHUP` é o terminal que fechou — ssh caído, aba fechada.
    const TERMINATING: [i32; 3] = [SIGINT, SIGTERM, SIGHUP];

    pub(super) fn install() -> io::Result<()> {
        let mut signals = Signals::new(TERMINATING)?;
        std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    super::restore();
                    // Termina como o sinal terminaria, e não com um código de saída: quem chamou
                    // — o shell, um supervisor — distingue "morto por SIGTERM" de "saiu com erro".
                    let _ = emulate_default_handler(signal);
                }
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entra na tela alternativa (DECSET 1049) e esconde o cursor (DECTCEM).
    const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l";
    /// Mostra o cursor e sai da tela alternativa — o inverso, na ordem inversa.
    const LEAVE: &[u8] = b"\x1b[?25h\x1b[?1049l";

    #[test]
    fn a_saida_da_tela_desfaz_a_entrada_na_ordem_inversa() {
        let mut out = Vec::new();
        enter_screen(&mut out).expect("escrever em memória não falha");
        leave_screen(&mut out).expect("escrever em memória não falha");
        assert_eq!(out, [ENTER, LEAVE].concat());
    }
}

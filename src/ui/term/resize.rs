//! Mudança de tamanho do terminal (RF-513).
//!
//! O aviso chega como evento do `crossterm`: no Unix ele vem do `SIGWINCH`, no Windows dos
//! eventos do console. Duas coisas nele não merecem confiança, e por isso a resposta é sempre
//! o tamanho perguntado ao terminal na hora:
//!
//! * a quantidade — arrastar a borda da janela dispara dezenas de avisos por segundo, e
//!   repintar a tela inteira para cada um gastaria a banda que o [`super::pace`] economiza;
//! * o número que vem junto — com vários avisos na fila, o primeiro já está velho.
//!
//! Por enquanto qualquer outro evento é descartado. As teclas passam a ser lidas aqui na
//! etapa 5.8, quando houver ações para elas.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use crossterm::terminal;

/// Largura e altura do terminal, em células.
pub type Size = (u16, u16);

/// Espera até `timeout` passar ou o terminal mudar de tamanho, o que vier antes.
///
/// Devolve o tamanho novo, ou `None` se o prazo acabou sem mudança. É onde o laço de desenho
/// espera o próximo quadro: acordar cedo num redimensionamento evita mostrar um quadro do
/// tamanho errado até o próximo tique.
pub fn wait(timeout: Duration) -> io::Result<Option<Size>> {
    wait_with(timeout, event::poll, event::read, terminal::size)
}

/// [`wait`] com as fontes de evento e de tamanho trocáveis, para os testes.
fn wait_with(
    timeout: Duration,
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut read: impl FnMut() -> io::Result<Event>,
    size: impl FnOnce() -> io::Result<Size>,
) -> io::Result<Option<Size>> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !poll(remaining)? {
            return Ok(None);
        }
        if matches!(read()?, Event::Resize(..)) {
            break;
        }
    }
    // Esvazia a fila sem esperar: os avisos que chegaram junto dizem a mesma coisa.
    while poll(Duration::ZERO)? {
        read()?;
    }
    size().map(Some)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crossterm::event::{KeyCode, KeyEvent};

    use super::*;

    const LONG: Duration = Duration::from_secs(60);

    /// Espera sobre uma fila roteirizada de eventos, com `current` como tamanho do terminal.
    ///
    /// Devolve o resultado e quantos eventos sobraram na fila.
    fn scripted(events: Vec<Event>, current: Size) -> (Option<Size>, usize) {
        let queue = std::cell::RefCell::new(VecDeque::from(events));
        let result = wait_with(
            LONG,
            |_| Ok(!queue.borrow().is_empty()),
            || {
                queue
                    .borrow_mut()
                    .pop_front()
                    .ok_or_else(|| io::Error::other("leitura com a fila vazia"))
            },
            || Ok(current),
        )
        .expect("a fila roteirizada não falha");
        let left = queue.borrow().len();
        (result, left)
    }

    fn key() -> Event {
        Event::Key(KeyEvent::from(KeyCode::Char('q')))
    }

    #[test]
    fn prazo_sem_evento_devolve_nada() {
        assert_eq!(scripted(vec![], (80, 24)), (None, 0));
    }

    #[test]
    fn rajada_de_avisos_vira_uma_resposta_com_o_tamanho_atual() {
        let burst = vec![
            Event::Resize(90, 30),
            Event::Resize(100, 35),
            Event::Resize(110, 40),
        ];
        // O número do primeiro aviso já está velho: vale o que o terminal diz agora.
        assert_eq!(scripted(burst, (120, 40)), (Some((120, 40)), 0));
    }

    #[test]
    fn outro_evento_nao_interrompe_a_espera() {
        assert_eq!(
            scripted(vec![key(), Event::Resize(1, 1), key()], (80, 24)),
            (Some((80, 24)), 0)
        );
        assert_eq!(scripted(vec![key(), key()], (80, 24)), (None, 0));
    }
}

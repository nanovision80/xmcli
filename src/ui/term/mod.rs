//! Superfície de desenho: modo raw, tela alternativa e cursor oculto (RF-515).
//!
//! São dois estados distintos, e por isso duas guardas:
//!
//! * o **modo raw** é do dispositivo de terminal — o processo inteiro o liga e desliga, sem
//!   escrever nada na saída ([`RawMode`]);
//! * a **tela alternativa** e o **cursor** são sequências de escape escritas na saída
//!   ([`Screen`]), e por isso cabem em qualquer `Write` — é assim que os testes as conferem sem
//!   terminal nenhum.
//!
//! [`Session`] junta as duas na ordem certa. Cada guarda desfaz o que fez quando sai de escopo,
//! inclusive por pânico que desenrole a pilha. Sinais e pânico com `abort` são a etapa 4.2.

use std::io::{self, Stdout, Write};

use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// Modo raw ligado enquanto a guarda existir.
///
/// No modo raw o terminal entrega cada tecla assim que ela chega, sem eco e sem esperar o
/// `Enter`, e para de traduzir `\n` em `\r\n` — é o que permite ao player desenhar célula a
/// célula.
#[derive(Debug)]
pub struct RawMode(());

impl RawMode {
    pub fn enable() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self(()))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Não há a quem informar a falha: quem está saindo de escopo pode ser um pânico, e o
        // terminal que não voltou não tem como mostrar mensagem nenhuma.
        let _ = disable_raw_mode();
    }
}

/// Tela alternativa com o cursor oculto, escritas em `out` enquanto a guarda existir.
///
/// A tela alternativa é o que devolve ao usuário, na saída, o terminal exatamente como estava
/// antes — o histórico de rolagem não recebe um único quadro do player.
#[derive(Debug)]
pub struct Screen<W: Write> {
    out: W,
}

impl<W: Write> Screen<W> {
    pub fn enter(mut out: W) -> io::Result<Self> {
        execute!(out, EnterAlternateScreen, Hide)?;
        Ok(Self { out })
    }

    /// Onde desenhar.
    pub fn out(&mut self) -> &mut W {
        &mut self.out
    }
}

impl<W: Write> Drop for Screen<W> {
    fn drop(&mut self) {
        // O inverso de `enter`, na ordem inversa. Falha ignorada pelo mesmo motivo de
        // `RawMode::drop`.
        let _ = execute!(self.out, Show, LeaveAlternateScreen);
    }
}

/// O terminal inteiro nas mãos do player: modo raw, tela alternativa e cursor oculto.
#[derive(Debug)]
pub struct Session {
    // A ordem dos campos é a ordem de restauração: o Rust descarta os campos na ordem em que
    // são declarados. A tela sai primeiro, ainda em modo raw, e só então o terminal volta ao
    // modo normal — o inverso da entrada.
    screen: Screen<Stdout>,
    _raw: RawMode,
}

impl Session {
    pub fn enter() -> io::Result<Self> {
        // O modo raw vem primeiro para que nenhuma tecla digitada durante a troca de tela
        // apareça em eco. Se a troca falhar, a guarda já existente desliga o modo raw.
        let raw = RawMode::enable()?;
        let screen = Screen::enter(io::stdout())?;
        Ok(Self { screen, _raw: raw })
    }

    /// Onde desenhar.
    pub fn out(&mut self) -> &mut Stdout {
        self.screen.out()
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
    fn a_tela_entra_e_sai_na_ordem_inversa() {
        let mut out = Vec::new();
        {
            let screen = Screen::enter(&mut out).expect("escrever em memória não falha");
            drop(screen);
        }
        assert_eq!(out, [ENTER, LEAVE].concat());
    }

    #[test]
    fn o_que_se_desenha_fica_entre_a_entrada_e_a_saida() {
        let mut out = Vec::new();
        {
            let mut screen = Screen::enter(&mut out).expect("escrever em memória não falha");
            screen
                .out()
                .write_all(b"quadro")
                .expect("escrever em memória não falha");
        }
        assert_eq!(out, [ENTER, b"quadro", LEAVE].concat());
    }

    #[test]
    fn um_panico_ainda_restaura_a_tela() {
        let out = std::sync::Mutex::new(Vec::new());
        let result = std::panic::catch_unwind(|| {
            let mut guard = out.lock().expect("ninguém mais usa o buffer");
            let _screen = Screen::enter(&mut *guard).expect("escrever em memória não falha");
            panic!("quadro com defeito");
        });

        assert!(result.is_err(), "o pânico atravessou a guarda");
        let out = out
            .into_inner()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(out, [ENTER, LEAVE].concat());
    }
}

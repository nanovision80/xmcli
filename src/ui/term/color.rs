//! Quantas cores o terminal aceita (RF-514).
//!
//! Não há como perguntar ao terminal: quem responde é o ambiente que ele deixa para o processo.
//! A detecção lê só variáveis, e por isso é uma função pura sobre elas — os testes passam o
//! ambiente que quiserem, sem terminal nenhum.
//!
//! Na dúvida, desce. Cor a menos deixa o palco mais pobre; cor a mais vira lixo na tela, ou
//! uma sequência de escape que o terminal imprime como texto.

use std::ffi::OsString;

/// Pedido explícito de saída sem cor, de qualquer programa (<https://no-color.org>).
const NO_COLOR: &str = "NO_COLOR";

/// Tipo de terminal, na convenção do terminfo.
const TERM: &str = "TERM";

/// Anúncio de cor que o terminfo não descreve; na prática, de truecolor.
const COLORTERM: &str = "COLORTERM";

/// Posta pelo Windows Terminal em toda sessão. No Windows o `TERM` não costuma existir, e esta
/// é a única pista de um terminal com truecolor.
const WT_SESSION: &str = "WT_SESSION";

/// `TERM` de um terminal que não interpreta sequência de escape nenhuma.
const TERM_DUMB: &str = "dumb";

/// Valores de `COLORTERM` que anunciam 24 bits por cor.
const COLORTERM_TRUECOLOR: [&str; 2] = ["truecolor", "24bit"];

/// Sufixo de `TERM` das entradas do terminfo com cor direta, como `xterm-direct`.
const TERM_DIRECT_SUFFIX: &str = "-direct";

/// Trecho de `TERM` das entradas com paleta de 256 cores, como `xterm-256color`.
const TERM_256_MARKER: &str = "256color";

/// Profundidade de cor que o palco e o cromo podem usar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// Sem cor: só atributos de texto.
    Mono,
    /// As 16 cores ANSI, que todo terminal com cor entende.
    Ansi16,
    /// A paleta de 256 cores do xterm.
    Ansi256,
    /// 24 bits por cor.
    TrueColor,
}

/// Decide a profundidade de cor a partir do ambiente.
///
/// `var` devolve o valor de uma variável, ou `None` se ela não existe. `mono` é o `--mono` da
/// linha de comando, que vence tudo: o usuário que pede monocromático sabe do próprio terminal
/// mais do que qualquer variável.
pub fn detect(var: impl Fn(&str) -> Option<OsString>, mono: bool) -> ColorDepth {
    // O NO_COLOR vale quando existe e não está vazio — é a definição do padrão, e uma variável
    // vazia costuma ser resto de `export NO_COLOR=` em script, não pedido.
    let no_color = var(NO_COLOR).is_some_and(|value| !value.is_empty());
    let term = var(TERM);
    let term = term.as_ref().and_then(|value| value.to_str()).unwrap_or("");
    if mono || no_color || term == TERM_DUMB {
        return ColorDepth::Mono;
    }

    let colorterm = var(COLORTERM);
    let colorterm = colorterm.as_ref().and_then(|value| value.to_str());
    if colorterm.is_some_and(|value| COLORTERM_TRUECOLOR.contains(&value))
        || term.ends_with(TERM_DIRECT_SUFFIX)
        || var(WT_SESSION).is_some()
    {
        return ColorDepth::TrueColor;
    }
    if term.contains(TERM_256_MARKER) {
        return ColorDepth::Ansi256;
    }
    ColorDepth::Ansi16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detecta com o ambiente dado e nenhuma outra variável.
    fn with_env(vars: &[(&str, &str)], mono: bool) -> ColorDepth {
        detect(
            |name| {
                vars.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            },
            mono,
        )
    }

    #[test]
    fn term_decide_entre_16_e_256() {
        assert_eq!(with_env(&[(TERM, "xterm")], false), ColorDepth::Ansi16);
        assert_eq!(
            with_env(&[(TERM, "xterm-256color")], false),
            ColorDepth::Ansi256
        );
        assert_eq!(
            with_env(&[(TERM, "tmux-256color")], false),
            ColorDepth::Ansi256
        );
    }

    #[test]
    fn colorterm_e_entrada_direct_anunciam_truecolor() {
        for colorterm in COLORTERM_TRUECOLOR {
            assert_eq!(
                with_env(&[(TERM, "xterm-256color"), (COLORTERM, colorterm)], false),
                ColorDepth::TrueColor
            );
        }
        assert_eq!(
            with_env(&[(TERM, "xterm-direct")], false),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn colorterm_desconhecido_nao_promove() {
        // Alguns terminais antigos põem "yes" ou o próprio nome em COLORTERM.
        assert_eq!(
            with_env(&[(TERM, "xterm-256color"), (COLORTERM, "yes")], false),
            ColorDepth::Ansi256
        );
    }

    #[test]
    fn windows_terminal_sem_term_tem_truecolor() {
        assert_eq!(
            with_env(&[(WT_SESSION, "a1b2")], false),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn ambiente_vazio_fica_nas_16_cores() {
        assert_eq!(with_env(&[], false), ColorDepth::Ansi16);
    }

    #[test]
    fn no_color_desliga_a_cor_so_quando_tem_valor() {
        let truecolor = [(TERM, "xterm-direct"), (NO_COLOR, "1")];
        assert_eq!(with_env(&truecolor, false), ColorDepth::Mono);
        assert_eq!(
            with_env(&[(TERM, "xterm-direct"), (NO_COLOR, "")], false),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn terminal_dumb_nao_recebe_cor_mesmo_anunciada() {
        assert_eq!(
            with_env(&[(TERM, TERM_DUMB), (COLORTERM, "truecolor")], false),
            ColorDepth::Mono
        );
    }

    #[test]
    fn mono_vence_qualquer_anuncio() {
        assert_eq!(
            with_env(&[(TERM, "xterm-256color"), (COLORTERM, "truecolor")], true),
            ColorDepth::Mono
        );
    }
}

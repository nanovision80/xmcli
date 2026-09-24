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

/// Profundidade de cor que o palco e o cromo podem usar, da menor para a maior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// Uma cor em 24 bits — o que o cromo e o palco pedem, antes de saber o terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
}

impl From<crate::config::Color> for Rgb {
    fn from(color: crate::config::Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

/// Uma cor já na forma que o terminal aceita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// A cor padrão do terminal: a única que existe sem cor.
    Default,
    /// Índice nas 16 cores ANSI, de 0 a 15.
    Ansi16(u8),
    /// Índice na paleta de 256 cores do xterm.
    Ansi256(u8),
    Rgb(Rgb),
}

impl ColorDepth {
    /// Um degrau abaixo na cascata de degradação do RF-621: truecolor → 256 → 16.
    ///
    /// Sem cor não é degrau: tirar a cor muda o que a interface comunica — botão ativo, canal
    /// mudo —, e isso só o usuário decide, com `--mono` ou `NO_COLOR`.
    pub fn lower(self) -> Option<Self> {
        match self {
            Self::TrueColor => Some(Self::Ansi256),
            Self::Ansi256 => Some(Self::Ansi16),
            Self::Ansi16 | Self::Mono => None,
        }
    }

    /// Um degrau acima na mesma cascata, o inverso de [`Self::lower`].
    pub fn raise(self) -> Option<Self> {
        match self {
            Self::Ansi16 => Some(Self::Ansi256),
            Self::Ansi256 => Some(Self::TrueColor),
            Self::TrueColor | Self::Mono => None,
        }
    }

    /// A cor mais próxima de `color` que esta profundidade consegue mostrar.
    pub fn ink(self, color: Rgb) -> Ink {
        match self {
            Self::Mono => Ink::Default,
            Self::Ansi16 => Ink::Ansi16(nearest_ansi16(color)),
            Self::Ansi256 => Ink::Ansi256(nearest_ansi256(color)),
            Self::TrueColor => Ink::Rgb(color),
        }
    }
}

/// As 16 cores ANSI com os valores padrão do xterm.
///
/// O terminal é livre para redefini-las, e o tema do usuário quase sempre redefine; estes são
/// só o ponto de referência para escolher a mais próxima.
const ANSI16_XTERM: [Rgb; 16] = [
    rgb(0, 0, 0),
    rgb(205, 0, 0),
    rgb(0, 205, 0),
    rgb(205, 205, 0),
    rgb(0, 0, 238),
    rgb(205, 0, 205),
    rgb(0, 205, 205),
    rgb(229, 229, 229),
    rgb(127, 127, 127),
    rgb(255, 0, 0),
    rgb(0, 255, 0),
    rgb(255, 255, 0),
    rgb(92, 92, 255),
    rgb(255, 0, 255),
    rgb(0, 255, 255),
    rgb(255, 255, 255),
];

/// Níveis de cada eixo do cubo 6×6×6 da paleta de 256 cores do xterm (`256colres.pl`).
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Índice da primeira cor do cubo: as 16 anteriores são as ANSI, que o tema redefine.
const CUBE_FIRST: u8 = 16;

/// Índice do primeiro cinza da rampa que fecha a paleta de 256.
const GRAY_FIRST: u8 = 232;

/// Quantos cinzas a rampa tem.
const GRAY_STEPS: u8 = 24;

/// Nível do primeiro cinza da rampa; os seguintes sobem de [`GRAY_STEP`] em [`GRAY_STEP`].
const GRAY_BASE: u8 = 8;

/// Distância entre dois cinzas vizinhos da rampa.
const GRAY_STEP: u8 = 10;

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// Quadrado da distância euclidiana entre duas cores.
///
/// Sem peso perceptual: o erro de escolher o vizinho errado numa paleta de 16 é pequeno perto
/// do erro do próprio tema, que redefine as cores.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let axis = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    axis(a.r, b.r) + axis(a.g, b.g) + axis(a.b, b.b)
}

fn nearest_ansi16(color: Rgb) -> u8 {
    (0..)
        .zip(ANSI16_XTERM)
        .min_by_key(|&(_, candidate)| distance(color, candidate))
        .map_or(0, |(index, _)| index)
}

/// O cubo e a rampa de cinzas competem, porque o cubo tem só seis cinzas e a rampa não tem
/// cor nenhuma: um cinza médio fica melhor na rampa, um vermelho, no cubo.
fn nearest_ansi256(color: Rgb) -> u8 {
    let (r, g, b) = (
        nearest_level(color.r),
        nearest_level(color.g),
        nearest_level(color.b),
    );
    let cube_color = rgb(CUBE_LEVELS[r], CUBE_LEVELS[g], CUBE_LEVELS[b]);
    let axis = CUBE_LEVELS.len();
    let cube_index = CUBE_FIRST + (r * axis * axis + g * axis + b) as u8;

    let (gray_index, gray_color) = (0..GRAY_STEPS)
        .map(|step| {
            let level = GRAY_BASE + step * GRAY_STEP;
            (GRAY_FIRST + step, rgb(level, level, level))
        })
        .min_by_key(|&(_, gray)| distance(color, gray))
        .unwrap_or((GRAY_FIRST, rgb(GRAY_BASE, GRAY_BASE, GRAY_BASE)));

    if distance(color, gray_color) < distance(color, cube_color) {
        gray_index
    } else {
        cube_index
    }
}

/// Posição do nível do cubo mais próximo de `value`.
fn nearest_level(value: u8) -> usize {
    (0..CUBE_LEVELS.len())
        .min_by_key(|&index| CUBE_LEVELS[index].abs_diff(value))
        .unwrap_or(0)
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

    #[test]
    fn cada_profundidade_mostra_a_cor_como_pode() {
        let red = rgb(255, 0, 0);
        assert_eq!(ColorDepth::Mono.ink(red), Ink::Default);
        assert_eq!(ColorDepth::Ansi16.ink(red), Ink::Ansi16(9));
        // 16 + 5·36: o vértice vermelho do cubo.
        assert_eq!(ColorDepth::Ansi256.ink(red), Ink::Ansi256(196));
        assert_eq!(ColorDepth::TrueColor.ink(red), Ink::Rgb(red));
    }

    #[test]
    fn dezesseis_cores_separam_normal_de_brilhante() {
        assert_eq!(nearest_ansi16(rgb(190, 10, 10)), 1);
        assert_eq!(nearest_ansi16(rgb(250, 90, 90)), 9);
        assert_eq!(nearest_ansi16(rgb(20, 20, 20)), 0);
    }

    #[test]
    fn cinza_medio_vai_para_a_rampa_e_cor_para_o_cubo() {
        // 8 + 12·10 = 128: exato na rampa, a 7 do nível 135 do cubo.
        assert_eq!(nearest_ansi256(rgb(128, 128, 128)), GRAY_FIRST + 12);
        // Os cantos do cubo são cores exatas.
        assert_eq!(nearest_ansi256(rgb(0, 0, 0)), CUBE_FIRST);
        assert_eq!(nearest_ansi256(rgb(255, 255, 255)), 231);
        // 95/135/175 em r/g/b: nível 1, 2 e 3.
        assert_eq!(nearest_ansi256(rgb(95, 135, 175)), 16 + 36 + 2 * 6 + 3);
    }
}

//! Do quadro aos bytes: diff contra o que está na tela, coalescência de SGR e um `write` por
//! quadro (RF-515, RF-604, RF-621).
//!
//! O diff é feito **depois** de converter a cor para o terminal. Duas cores de 24 bits que
//! caem no mesmo índice de 256 são a mesma cor para quem olha, e repintar a célula seria banda
//! gasta sem mudança visível — no palco, onde quase tudo muda de leve a cada quadro, é a
//! diferença entre caber no orçamento de RNF-07 e não caber.
//!
//! A cor corrente do terminal (a "caneta") também é lembrada entre quadros: o SGR só sai quando
//! a célula seguinte pede outra cor, esteja ela no mesmo quadro ou no próximo.

use std::io::{self, Write};

use super::buffer::Buffer;
use super::color::{ColorDepth, Ink};

/// Início de toda sequência de controle (ECMA-48, CSI).
const CSI: &[u8] = b"\x1b[";

/// Final de CUP: posiciona o cursor em linha;coluna, contadas a partir de 1.
const CUP_FINAL: u8 = b'H';

/// Final de SGR: muda os atributos de desenho.
const SGR_FINAL: u8 = b'm';

/// Separador de parâmetros numa sequência de controle.
const PARAM_SEPARATOR: u8 = b';';

/// SGR para a cor padrão do terminal, do texto e do fundo.
const SGR_FG_DEFAULT: u8 = 39;
const SGR_BG_DEFAULT: u8 = 49;

/// Primeiro SGR das 8 cores ANSI normais e das 8 brilhantes (aixterm), do texto e do fundo.
const SGR_FG_ANSI: u8 = 30;
const SGR_BG_ANSI: u8 = 40;
const SGR_FG_ANSI_BRIGHT: u8 = 90;
const SGR_BG_ANSI_BRIGHT: u8 = 100;

/// Quantas cores ANSI há em cada metade, normal e brilhante.
const ANSI_HALF: u8 = 8;

/// SGR estendido (ISO 8613-6): 38 para o texto, 48 para o fundo, seguido do modo — 5 para
/// índice de 256, 2 para RGB.
const SGR_FG_EXTENDED: u8 = 38;
const SGR_BG_EXTENDED: u8 = 48;
const SGR_EXTENDED_INDEXED: u8 = 5;
const SGR_EXTENDED_RGB: u8 = 2;

/// O que sai no lugar de um caractere de controle.
///
/// Todo texto vindo de módulo é hostil (CLAUDE.md §2, invariante 6): um ESC num título seria
/// uma sequência de escape escrita direto no terminal do usuário.
const CONTROL_REPLACEMENT: char = '\u{FFFD}';

/// Uma célula como o terminal a mostra: o caractere e as cores já convertidas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    ch: char,
    fg: Ink,
    bg: Ink,
}

/// Pinta quadros no terminal, lembrando o que já está na tela.
///
/// Os vetores internos crescem até o tamanho da tela no primeiro quadro e depois só são
/// reaproveitados: pintar não aloca enquanto a tela não mudar de tamanho.
#[derive(Debug)]
pub struct Painter {
    depth: ColorDepth,
    /// O que está na tela, com o tamanho dela. Vazio quando não se sabe — antes do primeiro
    /// quadro, ou depois de a tela mudar de tamanho.
    shown: Vec<Shown>,
    width: u16,
    /// A cor corrente do terminal, se conhecida.
    pen: Option<(Ink, Ink)>,
    /// Os bytes do quadro em montagem.
    bytes: Vec<u8>,
}

impl Painter {
    pub fn new(depth: ColorDepth) -> Self {
        Self {
            depth,
            shown: Vec::new(),
            width: 0,
            pen: None,
            bytes: Vec::new(),
        }
    }

    /// Muda a profundidade de cor dos próximos quadros (RF-621).
    ///
    /// Não precisa repintar nada à força: as células já mostradas guardam a cor na
    /// profundidade antiga, e o diff do próximo quadro encontra nelas a diferença.
    pub fn set_depth(&mut self, depth: ColorDepth) {
        self.depth = depth;
    }

    /// Esquece o que está na tela: o próximo quadro repinta todas as células.
    ///
    /// Para depois de um redimensionamento, mesmo que o tamanho tenha voltado ao de antes: o
    /// emulador pode ter rearrumado ou apagado o conteúdo no caminho.
    pub fn invalidate(&mut self) {
        self.shown.clear();
    }

    /// Escreve o quadro em `out` numa chamada só, e devolve quantos bytes ele custou.
    ///
    /// Um `write_all` e não uma série de escritas: o terminal que recebe meio quadro mostra
    /// meio quadro (RF-604).
    pub fn present(&mut self, frame: &Buffer, out: &mut impl Write) -> io::Result<usize> {
        let bytes = self.encode(frame);
        out.write_all(bytes)?;
        out.flush()?;
        Ok(bytes.len())
    }

    /// Os bytes que levam a tela do quadro anterior a `frame`.
    pub fn encode(&mut self, frame: &Buffer) -> &[u8] {
        self.bytes.clear();
        let cells = frame.cells();
        let resized = self.width != frame.width() || self.shown.len() != cells.len();
        if resized {
            self.width = frame.width();
            self.shown.clear();
            self.pen = None;
        }

        // Onde o cursor está, se é sabido: depois de escrever uma célula ele fica na seguinte,
        // e a célula vizinha dispensa posicionar.
        let mut cursor = None;
        for (index, cell) in cells.iter().enumerate() {
            let next = Shown {
                ch: printable(cell.ch),
                fg: self.depth.ink(cell.fg),
                bg: self.depth.ink(cell.bg),
            };
            if !resized && self.shown[index] == next {
                continue;
            }

            if cursor != Some(index) {
                self.move_to(index);
            }
            self.set_pen(next.fg, next.bg);
            let mut utf8 = [0; 4];
            self.bytes
                .extend_from_slice(next.ch.encode_utf8(&mut utf8).as_bytes());
            cursor = self.after(index);

            if resized {
                self.shown.push(next);
            } else {
                self.shown[index] = next;
            }
        }
        &self.bytes
    }

    /// A posição do cursor depois de escrever na célula `index`.
    ///
    /// Na última coluna o terminal não avança: fica esperando para quebrar a linha no próximo
    /// caractere, e o comportamento dessa espera varia. A célula seguinte, então, posiciona.
    fn after(&self, index: usize) -> Option<usize> {
        let column = index % usize::from(self.width) + 1;
        (column < usize::from(self.width)).then_some(index + 1)
    }

    fn move_to(&mut self, index: usize) {
        let width = usize::from(self.width);
        self.bytes.extend_from_slice(CSI);
        push_number(&mut self.bytes, index / width + 1);
        self.bytes.push(PARAM_SEPARATOR);
        push_number(&mut self.bytes, index % width + 1);
        self.bytes.push(CUP_FINAL);
    }

    /// Emite só a parte da cor que mudou, numa sequência só.
    fn set_pen(&mut self, fg: Ink, bg: Ink) {
        let (fg_changed, bg_changed) = match self.pen {
            Some((pen_fg, pen_bg)) => (pen_fg != fg, pen_bg != bg),
            None => (true, true),
        };
        self.pen = Some((fg, bg));
        if !fg_changed && !bg_changed {
            return;
        }

        self.bytes.extend_from_slice(CSI);
        if fg_changed {
            push_ink(&mut self.bytes, fg, Layer::Foreground);
        }
        if fg_changed && bg_changed {
            self.bytes.push(PARAM_SEPARATOR);
        }
        if bg_changed {
            push_ink(&mut self.bytes, bg, Layer::Background);
        }
        self.bytes.push(SGR_FINAL);
    }
}

#[derive(Clone, Copy)]
enum Layer {
    Foreground,
    Background,
}

/// Os parâmetros de SGR de uma cor, sem o CSI e sem o final.
fn push_ink(bytes: &mut Vec<u8>, ink: Ink, layer: Layer) {
    let foreground = matches!(layer, Layer::Foreground);
    let pick = |fg: u8, bg: u8| if foreground { fg } else { bg };
    match ink {
        Ink::Default => push_number(bytes, pick(SGR_FG_DEFAULT, SGR_BG_DEFAULT).into()),
        Ink::Ansi16(index) if index < ANSI_HALF => {
            push_number(bytes, (pick(SGR_FG_ANSI, SGR_BG_ANSI) + index).into());
        }
        Ink::Ansi16(index) => {
            let base = pick(SGR_FG_ANSI_BRIGHT, SGR_BG_ANSI_BRIGHT);
            push_number(bytes, (base + index - ANSI_HALF).into());
        }
        Ink::Ansi256(index) => {
            push_params(
                bytes,
                &[
                    pick(SGR_FG_EXTENDED, SGR_BG_EXTENDED),
                    SGR_EXTENDED_INDEXED,
                    index,
                ],
            );
        }
        Ink::Rgb(color) => {
            push_params(
                bytes,
                &[
                    pick(SGR_FG_EXTENDED, SGR_BG_EXTENDED),
                    SGR_EXTENDED_RGB,
                    color.r,
                    color.g,
                    color.b,
                ],
            );
        }
    }
}

fn push_params(bytes: &mut Vec<u8>, params: &[u8]) {
    for (position, &param) in params.iter().enumerate() {
        if position > 0 {
            bytes.push(PARAM_SEPARATOR);
        }
        push_number(bytes, param.into());
    }
}

/// Escreve um número em decimal, sem passar por `String`.
fn push_number(bytes: &mut Vec<u8>, value: usize) {
    // `write!` num `Vec` não falha: a única falha possível seria de alocação, que aborta.
    let _ = write!(bytes, "{value}");
}

fn printable(ch: char) -> char {
    if ch.is_control() {
        CONTROL_REPLACEMENT
    } else {
        ch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::term::buffer::Cell;
    use crate::ui::term::color::Rgb;

    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
    const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

    fn cell(ch: char, fg: Rgb, bg: Rgb) -> Cell {
        Cell { ch, fg, bg }
    }

    /// Uma linha com o texto dado, em vermelho sobre azul.
    fn line(text: &str) -> Buffer {
        let width = u16::try_from(text.chars().count()).expect("texto de teste é curto");
        let mut buffer = Buffer::new(width, 1);
        for (x, ch) in (0..).zip(text.chars()) {
            buffer.set(x, 0, cell(ch, RED, BLUE));
        }
        buffer
    }

    fn encode(painter: &mut Painter, frame: &Buffer) -> String {
        String::from_utf8(painter.encode(frame).to_vec()).expect("a saída é UTF-8")
    }

    #[test]
    fn primeiro_quadro_posiciona_uma_vez_e_pinta_a_cor_uma_vez() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        assert_eq!(
            encode(&mut painter, &line("abc")),
            "\x1b[1;1H\x1b[38;2;255;0;0;48;2;0;0;255mabc"
        );
    }

    #[test]
    fn quadro_igual_nao_custa_nada() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        let frame = line("abc");
        painter.encode(&frame);
        assert_eq!(encode(&mut painter, &frame), "");
    }

    #[test]
    fn so_a_celula_que_mudou_sai_e_a_caneta_se_mantem() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        painter.encode(&line("abcd"));
        assert_eq!(encode(&mut painter, &line("abXd")), "\x1b[1;3HX");
    }

    #[test]
    fn so_a_parte_da_cor_que_mudou_sai() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        let mut frame = line("ab");
        painter.encode(&frame);
        frame.set(1, 0, cell('b', BLUE, BLUE));
        assert_eq!(encode(&mut painter, &frame), "\x1b[1;2H\x1b[38;2;0;0;255mb");
    }

    #[test]
    fn cor_que_cai_no_mesmo_indice_nao_repinta() {
        let mut painter = Painter::new(ColorDepth::Ansi256);
        let mut frame = line("ab");
        painter.encode(&frame);
        // 250,5,5 e 255,0,0 são o mesmo vértice do cubo: para quem olha, nada mudou.
        frame.set(0, 0, cell('a', Rgb { r: 250, g: 5, b: 5 }, BLUE));
        assert_eq!(encode(&mut painter, &frame), "");
    }

    #[test]
    fn dezesseis_cores_usam_os_codigos_curtos() {
        // Vermelho puro cai no brilhante (9 → 91); azul puro, no normal do xterm (4 → 44).
        let mut painter = Painter::new(ColorDepth::Ansi16);
        assert_eq!(encode(&mut painter, &line("a")), "\x1b[1;1H\x1b[91;44ma");

        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let mut frame = Buffer::new(1, 1);
        frame.set(0, 0, cell('a', Rgb::BLACK, white));
        assert_eq!(encode(&mut painter, &frame), "\x1b[1;1H\x1b[30;107ma");
    }

    #[test]
    fn sem_cor_nao_sai_sgr_alem_do_padrao() {
        let mut painter = Painter::new(ColorDepth::Mono);
        assert_eq!(encode(&mut painter, &line("ab")), "\x1b[1;1H\x1b[39;49mab");
    }

    #[test]
    fn nova_linha_e_celula_distante_posicionam() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        let mut frame = Buffer::new(3, 2);
        painter.encode(&frame);
        frame.set(2, 0, cell('x', Rgb::BLACK, Rgb::BLACK));
        frame.set(0, 1, cell('y', Rgb::BLACK, Rgb::BLACK));
        // Depois da última coluna o cursor não é confiável, mesmo sendo a célula seguinte.
        assert_eq!(encode(&mut painter, &frame), "\x1b[1;3Hx\x1b[2;1Hy");
    }

    #[test]
    fn mudar_de_tamanho_repinta_tudo() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        painter.encode(&line("ab"));
        assert_eq!(
            encode(&mut painter, &line("abc")),
            "\x1b[1;1H\x1b[38;2;255;0;0;48;2;0;0;255mabc"
        );
    }

    #[test]
    fn invalidar_repinta_tudo_no_mesmo_tamanho() {
        let mut painter = Painter::new(ColorDepth::TrueColor);
        let frame = line("ab");
        painter.encode(&frame);
        painter.invalidate();
        assert_eq!(
            encode(&mut painter, &frame),
            "[1;1H[38;2;255;0;0;48;2;0;0;255mab"
        );
    }

    #[test]
    fn caractere_de_controle_nao_chega_ao_terminal() {
        let mut painter = Painter::new(ColorDepth::Mono);
        let text = encode(&mut painter, &line("a\x1bb"));
        assert_eq!(text, "\x1b[1;1H\x1b[39;49ma\u{FFFD}b");
    }

    #[test]
    fn present_escreve_o_quadro_numa_chamada() {
        /// Conta as chamadas a `write`.
        #[derive(Default)]
        struct Counting {
            writes: usize,
            bytes: Vec<u8>,
        }
        impl Write for Counting {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.writes += 1;
                self.bytes.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut painter = Painter::new(ColorDepth::TrueColor);
        let mut out = Counting::default();
        let written = painter
            .present(&line("abc"), &mut out)
            .expect("escrever em memória não falha");
        assert_eq!(out.writes, 1);
        assert_eq!(written, out.bytes.len());
    }
}

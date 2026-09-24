//! A moldura do player e o aviso de terminal pequeno.
//!
//! Em ASCII de 7 bits, porque é o modo padrão e sempre suficiente (§3.5 dos requisitos): a
//! moldura precisa aparecer inteira em qualquer terminal, com qualquer fonte.

use crate::config::Theme;
use crate::ui::chrome::layout::{Layout, MIN_HEIGHT, MIN_WIDTH, Rect, layout};
use crate::ui::term::buffer::{Buffer, Cell};
use crate::ui::term::color::Rgb;

/// Canto e cruzamento de linhas da moldura.
const CORNER: char = '+';
/// Linha horizontal da moldura.
const HORIZONTAL: char = '-';
/// Linha vertical da moldura.
const VERTICAL: char = '|';

/// Desenha o quadro inteiro do cromo: fundo, moldura e, se o terminal for pequeno, o aviso.
///
/// Pinta todas as células, de modo que o quadro anterior não precisa ser apagado antes.
pub fn draw(frame: &mut Buffer, theme: &Theme) {
    let background = Rgb::from(theme.background);
    let blank = Cell {
        ch: ' ',
        fg: background,
        bg: background,
    };
    fill(frame, blank);

    let border = Cell {
        ch: HORIZONTAL,
        fg: Rgb::from(theme.frame),
        bg: background,
    };
    match layout(frame.width(), frame.height()) {
        Layout::Full(regions) => {
            // Uma linha de moldura acima de cada parte, e a última embaixo de tudo.
            let last = frame.height() - 1;
            for y in [
                regions.header.y - 1,
                regions.stage.y - 1,
                regions.transport.y - 1,
                last,
            ] {
                horizontal(frame, y, border);
            }
            for rect in [regions.header, regions.stage, regions.transport] {
                sides(frame, rect, border);
            }
            let bottom = Rect {
                height: regions.status.y - regions.transport.y + 1,
                ..regions.transport
            };
            sides(frame, bottom, border);
        }
        Layout::TooSmall => {
            let text = Cell {
                fg: Rgb::from(theme.text),
                ..blank
            };
            let message = format!(
                "xmcli precisa de {MIN_WIDTH}x{MIN_HEIGHT}; este terminal tem {}x{}",
                frame.width(),
                frame.height()
            );
            write(frame, 0, 0, &message, text);
        }
    }
}

fn fill(frame: &mut Buffer, cell: Cell) {
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            frame.set(x, y, cell);
        }
    }
}

/// Uma linha de moldura de ponta a ponta, com cantos nas pontas.
fn horizontal(frame: &mut Buffer, y: u16, border: Cell) {
    for x in 0..frame.width() {
        frame.set(x, y, border);
    }
    let corner = Cell {
        ch: CORNER,
        ..border
    };
    frame.set(0, y, corner);
    frame.set(frame.width() - 1, y, corner);
}

/// As laterais da moldura ao lado de uma região.
fn sides(frame: &mut Buffer, rect: Rect, border: Cell) {
    let side = Cell {
        ch: VERTICAL,
        ..border
    };
    for y in rect.y..rect.y + rect.height {
        frame.set(rect.x - 1, y, side);
        frame.set(rect.x + rect.width, y, side);
    }
}

fn write(frame: &mut Buffer, x: u16, y: u16, text: &str, style: Cell) {
    for (offset, ch) in (0..).zip(text.chars()) {
        frame.set(x.saturating_add(offset), y, Cell { ch, ..style });
    }
}

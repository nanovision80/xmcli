//! A moldura do player, o cabeçalho e o aviso de terminal pequeno.
//!
//! Em ASCII de 7 bits, porque é o modo padrão e sempre suficiente (§3.5 dos requisitos): a
//! moldura precisa aparecer inteira em qualquer terminal, com qualquer fonte.

use std::time::Duration;

use crate::config::{Marquee, Theme, TimeDisplay};
use crate::ui::chrome::layout::{Layout, MIN_HEIGHT, MIN_WIDTH, Rect, layout};
use crate::ui::chrome::{marquee, seek, time, transport};
use crate::ui::term::buffer::{Buffer, Cell};
use crate::ui::term::color::Rgb;
use crate::ui::transport::Transport;

/// Canto e cruzamento de linhas da moldura.
const CORNER: char = '+';
/// Linha horizontal da moldura.
const HORIZONTAL: char = '-';
/// Linha vertical da moldura.
const VERTICAL: char = '|';

/// O que vem antes do título no cabeçalho, como no desenho do RF-500.
const HEADER_PREFIX: &str = " xmcli  >> ";
/// O que fecha o título no cabeçalho.
const HEADER_SUFFIX: &str = " << ";
/// Espaço entre a moldura e o que as linhas de baixo mostram.
const MARGIN: u16 = 1;
/// Espaço entre o tempo e os botões do transporte, como no desenho do RF-500.
const TIME_GAP: usize = 1;
// Os dois são ASCII, então cada byte é uma célula.
const PREFIX_WIDTH: u16 = HEADER_PREFIX.len() as u16;
const SUFFIX_WIDTH: u16 = HEADER_SUFFIX.len() as u16;

/// O que o quadro mostra além do tema.
pub struct View<'a> {
    /// O título que o marquee rola: título do módulo e nome do arquivo (RF-502).
    pub title: &'a str,
    pub marquee: &'a Marquee,
    /// Tempo desde que a faixa apareceu na tela; é o que move o marquee.
    pub elapsed: Duration,
    pub transport: Transport,
    /// Onde a música está dentro da faixa, em segundos: o que está soando, não o que foi
    /// mixado (RF-620).
    pub position: f64,
    /// Duração da faixa, em segundos.
    pub duration: f64,
    pub time: TimeDisplay,
}

/// Desenha o quadro inteiro do cromo: fundo, moldura, cabeçalho e, se o terminal for pequeno,
/// o aviso.
///
/// Pinta todas as células, de modo que o quadro anterior não precisa ser apagado antes.
pub fn draw(frame: &mut Buffer, theme: &Theme, view: &View) {
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
    let text = Cell {
        fg: Rgb::from(theme.text),
        ..blank
    };
    match layout(frame.width(), frame.height()) {
        Layout::Full(regions) => {
            header(frame, regions.header, view, text);
            let active = Cell {
                fg: Rgb::from(theme.active),
                ..text
            };
            let clock = time::display(view.time, view.position, view.duration);
            write(
                frame,
                regions.transport.x + MARGIN,
                regions.transport.y,
                &clock,
                text,
            );
            // Os botões começam depois da maior largura que o tempo pode ter nesta faixa.
            let after = MARGIN as usize + time::width(view.duration) + TIME_GAP;
            let buttons_rect = Rect {
                x: regions.transport.x.saturating_add(after as u16),
                ..regions.transport
            };
            buttons(frame, buttons_rect, view.transport, text, active);
            seek_bar(frame, regions.seek, view, text, active);
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
            let message = format!(
                "xmcli precisa de {MIN_WIDTH}x{MIN_HEIGHT}; este terminal tem {}x{}",
                frame.width(),
                frame.height()
            );
            write(frame, 0, 0, &message, text);
        }
    }
}

/// O cabeçalho: o nome do programa e o título rolando entre `>>` e `<<`.
fn header(frame: &mut Buffer, rect: Rect, view: &View, style: Cell) {
    let window = usize::from(rect.width.saturating_sub(PREFIX_WIDTH + SUFFIX_WIDTH));
    let len = view.title.chars().count();
    let hidden = marquee::offset(len, window, view.elapsed, view.marquee);
    let visible: String = view.title.chars().skip(hidden).take(window).collect();

    write(frame, rect.x, rect.y, HEADER_PREFIX, style);
    write(frame, rect.x + PREFIX_WIDTH, rect.y, &visible, style);
    // `window` saiu de `rect.width`, que é u16.
    let close = rect.x + PREFIX_WIDTH + window as u16;
    write(frame, close, rect.y, HEADER_SUFFIX, style);
}

/// Os botões do transporte, com o do estado atual na cor de destaque.
fn buttons(frame: &mut Buffer, rect: Rect, state: Transport, idle: Cell, active: Cell) {
    let mut x = rect.x;
    for piece in transport::buttons(state) {
        let style = if piece.active { active } else { idle };
        write(frame, x, rect.y, &piece.text, style);
        // Os botões são ASCII: cada byte é uma célula.
        x = x.saturating_add(piece.text.len() as u16);
    }
}

/// A barra de seek, com o ponto atual na cor de destaque e um espaço de margem de cada lado.
fn seek_bar(frame: &mut Buffer, rect: Rect, view: &View, style: Cell, knob: Cell) {
    let width = usize::from(rect.width.saturating_sub(MARGIN + MARGIN));
    let Some(bar) = seek::bar(width, view.position, view.duration) else {
        return;
    };
    let x = rect.x + MARGIN;
    write(frame, x, rect.y, &bar.text, style);
    // A barra é ASCII e cabe na região, que tem largura u16.
    let knob_x = x + bar.knob as u16;
    let ch = frame.get(knob_x, rect.y).map_or(' ', |cell| cell.ch);
    frame.set(knob_x, rect.y, Cell { ch, ..knob });
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

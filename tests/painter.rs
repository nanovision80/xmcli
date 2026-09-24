//! O que o `Painter` escreve, visto por um terminal (RF-515, RF-621).
//!
//! Os testes de unidade de `paint.rs` conferem bytes. Estes conferem a tela: o cartão de teste
//! em cada profundidade de cor, e o diff entre quadros chegando exatamente à mesma tela que
//! pintar do zero — inclusive através de um redimensionamento.

mod vt;

use xmcli::ui::term::buffer::{Buffer, Cell};
use xmcli::ui::term::color::{ColorDepth, Rgb};
use xmcli::ui::term::paint::Painter;

use vt::{Terminal, assert_snapshot, depth_label, show};

/// Todas as profundidades, da menor para a maior.
const DEPTHS: [ColorDepth; 4] = [
    ColorDepth::Mono,
    ColorDepth::Ansi16,
    ColorDepth::Ansi256,
    ColorDepth::TrueColor,
];

const WHITE: Rgb = rgb(255, 255, 255);
const NAVY: Rgb = rgb(0, 0, 128);
const PHOSPHOR: Rgb = rgb(0, 224, 64);

/// As barras do cartão: as cores de um cromo estilo Winamp, fortes e escuras.
const BARS: [Rgb; 8] = [
    rgb(230, 30, 30),
    rgb(240, 150, 20),
    rgb(230, 220, 40),
    rgb(40, 200, 60),
    rgb(30, 180, 200),
    rgb(40, 70, 220),
    rgb(170, 60, 210),
    rgb(90, 90, 100),
];

/// Degraus da rampa de cinza, do escuro ao claro.
const GRAY_STEPS: u8 = 8;

/// Título do cartão, com acento e travessão: texto que não é ASCII.
const TITLE: &str = " xmcli — cartão de teste ";

/// Glifos do CP437 que os módulos usam para desenhar.
const SHADES: &str = "░▒▓█";

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

fn write_text(buffer: &mut Buffer, x: u16, y: u16, text: &str, fg: Rgb, bg: Rgb) {
    for (offset, ch) in (0..).zip(text.chars()) {
        buffer.set(x.saturating_add(offset), y, Cell { ch, fg, bg });
    }
}

fn fill(buffer: &mut Buffer, rows: std::ops::Range<u16>, cell: impl Fn(u16) -> Cell) {
    for y in rows {
        for x in 0..buffer.width() {
            buffer.set(x, y, cell(x));
        }
    }
}

/// O cartão de teste: título, barras de cor, rampa de cinza e uma linha de glifos.
fn card(width: u16, height: u16) -> Buffer {
    let mut buffer = Buffer::new(width, height);
    let title_bar = Cell {
        bg: NAVY,
        ..Cell::BLANK
    };
    fill(&mut buffer, 0..1, |_| title_bar);
    write_text(&mut buffer, 0, 0, TITLE, WHITE, NAVY);

    let bars_end = height / 2;
    let bar_width = (width / BARS.len() as u16).max(1);
    fill(&mut buffer, 1..bars_end, |x| Cell {
        bg: BARS[usize::from(x / bar_width).min(BARS.len() - 1)],
        ..Cell::BLANK
    });

    let step_width = (width / u16::from(GRAY_STEPS)).max(1);
    let gray_step = u8::MAX / (GRAY_STEPS - 1);
    fill(&mut buffer, bars_end..height - 1, |x| {
        let step = (x / step_width).min(u16::from(GRAY_STEPS - 1)) as u8;
        let level = step * gray_step;
        Cell {
            ch: '▒',
            fg: WHITE,
            bg: rgb(level, level, level),
        }
    });

    let shades: Vec<char> = SHADES.chars().collect();
    fill(&mut buffer, height - 1..height, |x| Cell {
        ch: shades[usize::from(x) % shades.len()],
        fg: PHOSPHOR,
        bg: Rgb::BLACK,
    });
    buffer
}

#[test]
fn cartao_de_teste_em_cada_profundidade() {
    for depth in DEPTHS {
        let name = format!("cartao-80x24-{}", depth_label(depth));
        assert_snapshot(&name, &show(&card(80, 24), depth));
    }
}

#[test]
fn diff_entre_quadros_chega_a_mesma_tela_que_pintar_do_zero() {
    for depth in DEPTHS {
        let mut painter = Painter::new(depth);
        let mut terminal = Terminal::new(80, 24);

        let first = card(80, 24);
        terminal.feed(painter.encode(&first));

        let mut second = first.clone();
        write_text(&mut second, 2, 3, "tocando", WHITE, BARS[0]);
        write_text(&mut second, 70, 20, "00:42", PHOSPHOR, Rgb::BLACK);
        terminal.feed(painter.encode(&second));
        assert_eq!(terminal.snapshot(depth_label(depth)), show(&second, depth));

        // A janela cresce: o terminal muda de tamanho e o quadro seguinte vem com o novo.
        terminal.resize(100, 30);
        painter.invalidate();
        let mut third = card(100, 30);
        write_text(&mut third, 1, 28, "reflow", WHITE, Rgb::BLACK);
        terminal.feed(painter.encode(&third));
        assert_eq!(terminal.snapshot(depth_label(depth)), show(&third, depth));
    }
}

#[test]
fn texto_hostil_nao_comanda_o_terminal() {
    let mut frame = card(80, 24);
    // "Apague a tela e vá para o canto": o que um título malicioso tentaria.
    write_text(&mut frame, 0, 0, "\x1b[2J\x1b[H", WHITE, NAVY);
    let screen = show(&frame, ColorDepth::TrueColor);

    assert!(
        screen.contains("|?[2J?[H— cartão de teste"),
        "o ESC deveria virar ?, sem deslocar o resto da linha:\n{screen}"
    );
    // O resto da tela continua lá: nada foi apagado.
    assert!(screen.contains(SHADES), "a última linha sumiu:\n{screen}");
}

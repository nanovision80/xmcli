//! O cromo do player visto por um terminal (RF-501, RF-513, CLAUDE.md §7).
//!
//! A matriz da §7: os três tamanhos de referência em truecolor, e o menor deles em cada
//! profundidade de cor. E o terminal abaixo do mínimo, que ainda não tem modo shade.

mod vt;

use serde_json::json;
use xmcli::config::{self, Theme};
use xmcli::ui::chrome::frame;
use xmcli::ui::term::buffer::Buffer;
use xmcli::ui::term::color::ColorDepth;

use vt::{assert_snapshot, depth_label, show};

/// Tamanhos de referência da §7: o mínimo, um comum e um grande.
const SIZES: [(u16, u16); 3] = [(80, 24), (120, 40), (200, 60)];

fn theme() -> Theme {
    let settings = config::resolve(None, std::iter::empty(), &json!({}))
        .expect("a configuração padrão é válida");
    config::theme(&settings)
}

fn drawn(width: u16, height: u16) -> Buffer {
    let mut buffer = Buffer::new(width, height);
    frame::draw(&mut buffer, &theme());
    buffer
}

#[test]
fn moldura_nos_tamanhos_de_referencia() {
    for (width, height) in SIZES {
        let depth = ColorDepth::TrueColor;
        let name = format!("moldura-{width}x{height}-{}", depth_label(depth));
        assert_snapshot(&name, &show(&drawn(width, height), depth));
    }
}

#[test]
fn moldura_em_cada_profundidade() {
    for depth in [ColorDepth::Mono, ColorDepth::Ansi16, ColorDepth::Ansi256] {
        let name = format!("moldura-80x24-{}", depth_label(depth));
        assert_snapshot(&name, &show(&drawn(80, 24), depth));
    }
}

#[test]
fn terminal_pequeno_avisa_o_minimo() {
    assert_snapshot(
        "pequeno-79x24-truecolor",
        &show(&drawn(79, 24), ColorDepth::TrueColor),
    );
}

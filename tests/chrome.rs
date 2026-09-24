//! O cromo do player visto por um terminal (RF-501 a RF-504, RF-513, CLAUDE.md §7).
//!
//! A matriz da §7: os três tamanhos de referência em truecolor, e o menor deles em cada
//! profundidade de cor. O marquee no meio da rolagem, o transporte pausado sem cor nenhuma, a
//! barra de seek no meio da faixa e o terminal abaixo do mínimo, que ainda não tem modo shade.

mod vt;

use std::time::Duration;

use serde_json::json;
use xmcli::config::{self, Settings};
use xmcli::ui::chrome::frame::{self, View};
use xmcli::ui::chrome::marquee;
use xmcli::ui::term::buffer::Buffer;
use xmcli::ui::term::color::ColorDepth;
use xmcli::ui::transport::Transport;

use vt::{assert_snapshot, depth_label, show};

/// Tamanhos de referência da §7: o mínimo, um comum e um grande.
const SIZES: [(u16, u16); 3] = [(80, 24), (120, 40), (200, 60)];

/// Título longo o bastante para rolar em 80 colunas e curto o bastante para caber em 200.
const TITLE: &str = "Space Debris by Captain of Image, 1991. Greetings to all trackers";

/// Duração da faixa desenhada, em segundos.
const DURATION: f64 = 297.0;

/// Arquivo de onde o título veio.
const FILE: &str = "space_debris.mod";

fn settings() -> Settings {
    config::resolve(None, std::iter::empty(), &json!({})).expect("a configuração padrão é válida")
}

/// O quadro de uma faixa de [`DURATION`] segundos que está em `position`.
fn drawn_at(
    width: u16,
    height: u16,
    elapsed: Duration,
    transport: Transport,
    position: f64,
) -> Buffer {
    let settings = settings();
    let title = marquee::text(TITLE, FILE);
    let view = View {
        title: &title,
        marquee: &settings.ui.marquee,
        elapsed,
        transport,
        position,
        duration: DURATION,
    };
    let mut buffer = Buffer::new(width, height);
    frame::draw(&mut buffer, &config::theme(&settings), &view);
    buffer
}

fn drawn(width: u16, height: u16) -> Buffer {
    drawn_at(width, height, Duration::ZERO, Transport::Playing, 0.0)
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
fn marquee_no_meio_e_no_fim_da_rolagem() {
    let marquee = settings().ui.marquee;
    let pause = Duration::from_millis(u64::from(marquee.pause_ms));
    let step = Duration::from_secs(1) / u32::from(marquee.chars_per_second);
    let depth = ColorDepth::TrueColor;

    // Cinco passos depois da pausa inicial, o título andou cinco caracteres.
    let moving = drawn_at(80, 24, pause + step * 5, Transport::Playing, 0.0);
    assert_snapshot("marquee-rolando-80x24", &show(&moving, depth));
    // Em 80 colunas o texto tem 22 posições a percorrer. Trinta passos depois da pausa
    // inicial ele está na pausa final, com o fim encostado no `<<`.
    let end = drawn_at(80, 24, pause + step * 30, Transport::Playing, 0.0);
    assert_snapshot("marquee-no-fim-80x24", &show(&end, depth));
}

#[test]
fn transporte_pausado_aparece_sem_cor() {
    // Sem cor, só os parênteses dizem qual botão está ativo.
    let paused = drawn_at(80, 24, Duration::ZERO, Transport::Paused, 0.0);
    assert_snapshot(
        "transporte-pausado-80x24-mono",
        &show(&paused, ColorDepth::Mono),
    );
}

#[test]
fn seek_mostra_a_posicao_na_faixa() {
    // Um terço da faixa: um terço da barra já tocado, o ponto na cor de destaque.
    let third = drawn_at(80, 24, Duration::ZERO, Transport::Playing, DURATION / 3.0);
    assert_snapshot(
        "seek-um-terco-80x24-truecolor",
        &show(&third, ColorDepth::TrueColor),
    );
}

#[test]
fn terminal_pequeno_avisa_o_minimo() {
    assert_snapshot(
        "pequeno-79x24-truecolor",
        &show(&drawn(79, 24), ColorDepth::TrueColor),
    );
}

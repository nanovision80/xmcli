//! Harness de snapshot de terminal para os testes de UI (CLAUDE.md §7).
//!
//! Os bytes que o [`Painter`] escreve passam por um emulador de terminal independente, o
//! `vt100`, e o que se fotografa é a tela dele: o que um terminal de verdade mostraria. Assim o
//! teste confere a saída contra outra implementação de VT100/xterm, e não contra a ideia que o
//! próprio código faz dela.
//!
//! A foto é texto, para ser revisada no diff do PR:
//!
//! ```text
//! 80x24 truecolor
//! --- texto
//! |cada linha entre barras, com os espaços do fim visíveis|
//! --- cores
//! |um símbolo por célula, um símbolo por par de cores|
//! --- legenda
//! a  texto rgb(255,255,255)  fundo idx(4)
//! ```
//!
//! As referências ficam em `tests/snapshots/`. Para criar ou atualizar, rode os testes com
//! `XMCLI_ATUALIZAR_SNAPSHOTS=1` e revise o diff: referência ausente falha, nunca é criada
//! calada — senão o CI passaria sem conferir nada.

#![allow(dead_code)] // Cada arquivo de teste usa uma parte do harness.

use std::fmt::Write as _;
use std::path::PathBuf;

use xmcli::ui::term::buffer::Buffer;
use xmcli::ui::term::color::ColorDepth;
use xmcli::ui::term::paint::Painter;

/// Variável que troca a conferência pela gravação das referências.
const UPDATE_VAR: &str = "XMCLI_ATUALIZAR_SNAPSHOTS";

/// Diretório das referências, relativo à raiz do crate.
const SNAPSHOT_DIR: &str = "tests/snapshots";

/// Extensão dos arquivos de referência.
const SNAPSHOT_EXTENSION: &str = "txt";

/// O emulador não precisa de histórico: a tela alternativa não rola.
const SCROLLBACK: usize = 0;

/// Símbolos da legenda de cores, na ordem em que os pares aparecem na tela.
const LEGEND_SYMBOLS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Moldura de cada linha, para que os espaços do fim apareçam no arquivo e no diff.
const ROW_EDGE: char = '|';

/// Uma tela de terminal emulada.
pub struct Terminal {
    parser: vt100::Parser,
}

impl Terminal {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, SCROLLBACK),
        }
    }

    /// Entrega bytes ao terminal, como o `write` entregaria.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Muda o tamanho da tela, como o usuário arrastando a janela.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.parser.screen_mut().set_size(height, width);
    }

    /// A tela, no formato de snapshot. `label` é a primeira linha — em geral, a profundidade.
    pub fn snapshot(&self, label: &str) -> String {
        let screen = self.parser.screen();
        let (height, width) = screen.size();
        let mut text = String::new();
        let mut colors = String::new();
        // Busca linear: a legenda tem no máximo um par por símbolo, e `vt100::Color` não é
        // `Hash`.
        let mut legend: Vec<(vt100::Color, vt100::Color)> = Vec::new();

        for row in 0..height {
            text.push(ROW_EDGE);
            colors.push(ROW_EDGE);
            for col in 0..width {
                let cell = screen
                    .cell(row, col)
                    .expect("linha e coluna estão dentro da tela");
                let contents = cell.contents();
                text.push_str(if contents.is_empty() { " " } else { contents });

                let pair = (cell.fgcolor(), cell.bgcolor());
                let position = legend.iter().position(|&known| known == pair);
                let position = position.unwrap_or_else(|| {
                    legend.push(pair);
                    legend.len() - 1
                });
                let symbol = LEGEND_SYMBOLS.chars().nth(position).unwrap_or_else(|| {
                    panic!("mais de {} pares de cores na tela", LEGEND_SYMBOLS.len())
                });
                colors.push(symbol);
            }
            text.push(ROW_EDGE);
            text.push('\n');
            colors.push(ROW_EDGE);
            colors.push('\n');
        }

        let mut out = format!("{width}x{height} {label}\n--- texto\n{text}--- cores\n{colors}");
        out.push_str("--- legenda\n");
        for (symbol, (fg, bg)) in LEGEND_SYMBOLS.chars().zip(&legend) {
            let _ = writeln!(out, "{symbol}  texto {}  fundo {}", name(*fg), name(*bg));
        }
        out
    }
}

fn name(color: vt100::Color) -> String {
    match color {
        vt100::Color::Default => "padrão".to_owned(),
        vt100::Color::Idx(index) => format!("idx({index})"),
        vt100::Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
    }
}

/// Rótulo curto de uma profundidade, para a primeira linha e o nome do arquivo.
pub fn depth_label(depth: ColorDepth) -> &'static str {
    match depth {
        ColorDepth::Mono => "mono",
        ColorDepth::Ansi16 => "16",
        ColorDepth::Ansi256 => "256",
        ColorDepth::TrueColor => "truecolor",
    }
}

/// O que um terminal limpo mostra depois de receber `frame` pintado do zero.
pub fn show(frame: &Buffer, depth: ColorDepth) -> String {
    let mut terminal = Terminal::new(frame.width(), frame.height());
    terminal.feed(Painter::new(depth).encode(frame));
    terminal.snapshot(depth_label(depth))
}

/// Confere `actual` contra a referência `name`, ou a grava com [`UPDATE_VAR`].
pub fn assert_snapshot(name: &str, actual: &str) {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), SNAPSHOT_DIR]
        .iter()
        .collect::<PathBuf>()
        .join(name)
        .with_extension(SNAPSHOT_EXTENSION);

    if std::env::var_os(UPDATE_VAR).is_some() {
        std::fs::create_dir_all(path.parent().expect("o caminho tem diretório"))
            .expect("criar o diretório de snapshots");
        std::fs::write(&path, actual).expect("gravar o snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "sem referência em {} ({error}); rode com {UPDATE_VAR}=1 e revise o diff",
            path.display()
        )
    });
    // O checkout no Windows pode trazer CRLF; a foto é comparada pelo conteúdo, não pelo fim
    // de linha.
    let expected = expected.replace("\r\n", "\n");
    assert!(
        expected == actual,
        "a tela difere de {}\n--- esperado\n{expected}\n--- obtido\n{actual}",
        path.display()
    );
}

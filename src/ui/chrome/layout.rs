//! Onde fica cada parte do player, dado o tamanho do terminal (RF-501, RF-513).
//!
//! O desenho segue o do RF-500: cabeçalho em cima, palco no meio com toda a altura que
//! sobra, e embaixo transporte, barra de seek e status, cada parte separada por uma linha de
//! moldura.
//!
//! ```text
//! +------------------------------------------+
//! | cabeçalho                                |
//! +------------------------------------------+
//! | palco                                    |
//! +------------------------------------------+
//! | transporte                               |
//! | seek                                     |
//! | status                                   |
//! +------------------------------------------+
//! ```
//!
//! O cromo tem altura fixa; o que muda com o terminal é o palco. Abaixo do mínimo não há
//! layout que sirva, e o player cai para o modo de uma linha (RF-511, etapa 5.10).

/// Menor terminal em que o player completo funciona (RF-513).
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// Largura da moldura em cada lado.
const EDGE: u16 = 1;

/// Linhas do cabeçalho: marquee e modo do palco.
const HEADER_ROWS: u16 = 1;

/// Linhas embaixo do palco: transporte, seek e status.
const BOTTOM_ROWS: u16 = 3;

/// Linhas de moldura: em cima, entre cabeçalho e palco, entre palco e transporte, embaixo.
const FRAME_ROWS: u16 = 4;

/// Um retângulo de células.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    fn row(x: u16, y: u16, width: u16) -> Self {
        Self {
            x,
            y,
            width,
            height: 1,
        }
    }
}

/// As regiões do player completo. Nenhuma inclui a moldura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regions {
    pub header: Rect,
    pub stage: Rect,
    pub transport: Rect,
    pub seek: Rect,
    pub status: Rect,
}

/// Como o player ocupa o terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Full(Regions),
    /// Menor que [`MIN_WIDTH`] × [`MIN_HEIGHT`].
    TooSmall,
}

pub fn layout(width: u16, height: u16) -> Layout {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return Layout::TooSmall;
    }
    // Moldura dos dois lados.
    let inner = width - EDGE - EDGE;
    let stage_rows = height - HEADER_ROWS - BOTTOM_ROWS - FRAME_ROWS;

    // Cada região fica uma linha abaixo da anterior, pulando a moldura entre elas.
    let header_y = EDGE;
    let stage_y = header_y + HEADER_ROWS + EDGE;
    let transport_y = stage_y + stage_rows + EDGE;
    let seek_y = transport_y + 1;
    let status_y = seek_y + 1;
    Layout::Full(Regions {
        header: Rect::row(EDGE, header_y, inner),
        stage: Rect {
            x: EDGE,
            y: stage_y,
            width: inner,
            height: stage_rows,
        },
        transport: Rect::row(EDGE, transport_y, inner),
        seek: Rect::row(EDGE, seek_y, inner),
        status: Rect::row(EDGE, status_y, inner),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions(width: u16, height: u16) -> Regions {
        match layout(width, height) {
            Layout::Full(regions) => regions,
            Layout::TooSmall => panic!("{width}x{height} deveria caber"),
        }
    }

    #[test]
    fn no_minimo_o_palco_fica_com_o_que_sobra() {
        let regions = regions(MIN_WIDTH, MIN_HEIGHT);
        assert_eq!(regions.header, Rect::row(1, 1, 78));
        assert_eq!(
            regions.stage,
            Rect {
                x: 1,
                y: 3,
                width: 78,
                height: 16
            }
        );
        assert_eq!(regions.transport, Rect::row(1, 20, 78));
        assert_eq!(regions.seek, Rect::row(1, 21, 78));
        // A última linha do terminal é moldura.
        assert_eq!(regions.status, Rect::row(1, 22, 78));
    }

    #[test]
    fn terminal_maior_so_aumenta_o_palco() {
        let small = regions(MIN_WIDTH, MIN_HEIGHT);
        let large = regions(200, 60);
        assert_eq!(large.stage.height, small.stage.height + 36);
        assert_eq!(large.status.y, 58);
        assert_eq!(large.header.width, 198);
    }

    #[test]
    fn abaixo_do_minimo_nao_ha_layout() {
        assert_eq!(layout(MIN_WIDTH - 1, MIN_HEIGHT), Layout::TooSmall);
        assert_eq!(layout(MIN_WIDTH, MIN_HEIGHT - 1), Layout::TooSmall);
        assert_eq!(layout(0, 0), Layout::TooSmall);
    }
}

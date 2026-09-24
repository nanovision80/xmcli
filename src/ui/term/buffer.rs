//! A grade de células onde o cromo e o palco desenham um quadro (RF-515).
//!
//! A cor fica em 24 bits aqui: quem desenha não sabe nem precisa saber o terminal. A conversão
//! para o que ele aceita acontece uma vez, ao pintar ([`super::paint`]).

use super::color::Rgb;

/// Uma posição da tela: um caractere, com a cor dele e a do fundo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Cell {
    /// Célula vazia: espaço sobre preto.
    pub const BLANK: Self = Self {
        ch: ' ',
        fg: Rgb::BLACK,
        bg: Rgb::BLACK,
    };
}

/// Um quadro inteiro, linha a linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Buffer {
    /// Um quadro de `width` × `height` células vazias.
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::BLANK; usize::from(width) * usize::from(height)],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    /// Escreve uma célula. Fora da grade não faz nada: desenhar além da borda é recortar, não
    /// erro — é o que acontece com todo texto comprido numa tela pequena.
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if let Some(index) = self.index(x, y) {
            self.cells[index] = cell;
        }
    }

    pub fn get(&self, x: u16, y: u16) -> Option<Cell> {
        self.index(x, y).map(|index| self.cells[index])
    }

    /// Todas as células, linha a linha.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.width && y < self.height)
            .then(|| usize::from(y) * usize::from(self.width) + usize::from(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escrever_fora_da_grade_recorta() {
        let mut buffer = Buffer::new(2, 2);
        let cell = Cell {
            ch: 'x',
            ..Cell::BLANK
        };
        buffer.set(2, 0, cell);
        buffer.set(0, 2, cell);
        assert!(buffer.cells().iter().all(|&c| c == Cell::BLANK));

        buffer.set(1, 1, cell);
        assert_eq!(buffer.get(1, 1), Some(cell));
        assert_eq!(buffer.cells()[3], cell);
        assert_eq!(buffer.get(2, 1), None);
    }
}

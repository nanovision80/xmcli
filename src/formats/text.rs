//! Conversão dos textos do módulo para UTF-8 (RF-150).
//!
//! Nomes de amostra e mensagens não são UTF-8: trackers de PC gravam em **CP437** e os do
//! Amiga em ISO-8859-1. Ler como UTF-8 destrói a arte ASCII embutida nos nomes de amostra,
//! que é metade da graça de abrir um módulo antigo.

/// Glifos de CP437 para os bytes 0x80–0xFF.
const HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', ' ',
];

/// Glifos de CP437 para os bytes de controle 0x00–0x1F.
///
/// Autores usavam esses códigos como desenho (♪ ♫ ► ▲) dentro dos nomes de amostra, então
/// eles viram glifo — menos os que funcionam como espaço em branco de verdade.
const CONTROL: [char; 32] = [
    ' ', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕',
    '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

/// Glifo de CP437 para 0x7F, a "casinha".
const HOUSE: char = '⌂';

/// Converte um campo de nome de tamanho fixo.
///
/// O preenchimento com NUL e espaço à direita é descartado; o resto vira glifo, inclusive os
/// códigos de controle usados como desenho.
pub fn decode_name(raw: &[u8]) -> String {
    let trimmed = raw.split(|byte| *byte == 0).next().unwrap_or(raw);
    let text: String = trimmed.iter().map(|byte| glyph(*byte)).collect();
    text.trim_end().to_owned()
}

/// Converte a mensagem embutida, preservando as quebras de linha.
///
/// O Impulse Tracker termina cada linha com CR; o FastTracker usa CR/LF. Os dois viram `\n`.
pub fn decode_message(raw: &[u8]) -> String {
    let trimmed = raw.split(|byte| *byte == 0).next().unwrap_or(raw);
    let mut text = String::with_capacity(trimmed.len());
    let mut previous_was_cr = false;
    for byte in trimmed {
        match byte {
            b'\r' => text.push('\n'),
            b'\n' if previous_was_cr => {}
            b'\n' => text.push('\n'),
            other => text.push(glyph(*other)),
        }
        previous_was_cr = *byte == b'\r';
    }
    text.trim_end().to_owned()
}

fn glyph(byte: u8) -> char {
    match byte {
        b'\t' | b'\n' | b'\r' => ' ',
        0x00..=0x1F => CONTROL[byte as usize],
        0x20..=0x7E => byte as char,
        0x7F => HOUSE,
        _ => HIGH[(byte - 0x80) as usize],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_puro_passa_intacto() {
        assert_eq!(decode_name(b"space debris"), "space debris");
    }

    #[test]
    fn preenchimento_e_descartado() {
        assert_eq!(decode_name(b"nome\0\0lixo depois do nul"), "nome");
        assert_eq!(decode_name(b"nome       "), "nome");
    }

    #[test]
    fn arte_ascii_de_cp437_sobrevive() {
        // 0xDB 0xDC 0xB0 são os blocos usados em quase toda moldura de nome de amostra.
        assert_eq!(decode_name(&[0xDB, 0xDC, 0xB0]), "█▄░");
    }

    #[test]
    fn glifos_de_controle_viram_desenho() {
        // 0x0E é a colcheia dupla, comum em nomes de amostra.
        assert_eq!(decode_name(&[0x0E]), "♫");
    }

    #[test]
    fn mensagem_normaliza_quebras_de_linha() {
        assert_eq!(
            decode_message(b"linha um\rlinha dois\r\nlinha tres"),
            "linha um\nlinha dois\nlinha tres"
        );
    }

    #[test]
    fn entrada_vazia_vira_texto_vazio() {
        assert_eq!(decode_name(b""), "");
        assert_eq!(decode_message(b""), "");
    }
}

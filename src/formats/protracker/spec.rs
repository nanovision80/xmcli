// Constantes do formato MOD (ProTracker e derivados).
//
// Vêm da especificação do formato, não da configuração: ajustá-las quebra a leitura de
// arquivos reais (CLAUDE.md §4).

/// Título do módulo: 20 bytes no início do arquivo.
pub const TITLE_OFFSET: usize = 0;
pub const TITLE_LEN: usize = 20;

/// Tabela de amostras logo após o título.
pub const SAMPLE_TABLE_OFFSET: usize = TITLE_OFFSET + TITLE_LEN;
pub const SAMPLE_ENTRY_LEN: usize = 30;
pub const SAMPLE_NAME_LEN: usize = 22;
/// Comprimento da amostra, em *words*, no início da entrada, em big-endian (Amiga).
pub const SAMPLE_LENGTH_OFFSET: usize = SAMPLE_NAME_LEN;

/// Variante com assinatura de quatro letras, 31 amostras.
pub const SAMPLES_TAGGED: usize = 31;
/// Variante original do Ultimate Soundtracker, 15 amostras e sem assinatura.
pub const SAMPLES_TAGLESS: usize = 15;

pub const ORDER_TABLE_LEN: usize = 128;
/// Assinatura de quatro letras, logo após a tabela de ordens da variante com 31 amostras.
pub const TAG_LEN: usize = 4;

/// Canais da variante sem assinatura: o Amiga tinha quatro vozes.
pub const TAGLESS_CHANNELS: u16 = 4;

/// Deslocamento do byte de comprimento da sequência, para a contagem de amostras dada.
pub const fn song_length_offset(samples: usize) -> usize {
    SAMPLE_TABLE_OFFSET + samples * SAMPLE_ENTRY_LEN
}

/// Deslocamento da tabela de ordens (vem após o comprimento e o byte de reinício).
pub const fn order_table_offset(samples: usize) -> usize {
    song_length_offset(samples) + 2
}

/// Deslocamento da assinatura na variante com 31 amostras.
pub const fn tag_offset() -> usize {
    order_table_offset(SAMPLES_TAGGED) + ORDER_TABLE_LEN
}

/// Assinaturas de quatro canais.
const TAGS_4: [&[u8; TAG_LEN]; 6] = [b"M.K.", b"M!K!", b"M&K!", b"N.T.", b"FLT4", b"4CHN"];
/// Assinaturas de oito canais.
const TAGS_8: [&[u8; TAG_LEN]; 4] = [b"FLT8", b"8CHN", b"OCTA", b"CD81"];

/// Traduz a assinatura em número de canais, ou `None` se não for uma assinatura conhecida.
pub fn channels_from_tag(tag: &[u8]) -> Option<u16> {
    let tag: &[u8; TAG_LEN] = tag.try_into().ok()?;
    if TAGS_4.contains(&tag) {
        return Some(4);
    }
    if TAGS_8.contains(&tag) {
        return Some(8);
    }
    // "6CHN", "8CHN" … "9CHN" e "TDZ1".."TDZ3": um dígito antes do sufixo.
    if tag[1..] == *b"CHN" && tag[0].is_ascii_digit() {
        return digit(tag[0]);
    }
    if tag[..3] == *b"TDZ" && tag[3].is_ascii_digit() {
        return digit(tag[3]);
    }
    // "10CH".."32CH": dois dígitos antes do sufixo.
    if tag[2..] == *b"CH" && tag[0].is_ascii_digit() && tag[1].is_ascii_digit() {
        let tens = digit(tag[0])?;
        let units = digit(tag[1])?;
        return Some(tens * 10 + units);
    }
    None
}

fn digit(byte: u8) -> Option<u16> {
    byte.is_ascii_digit().then(|| u16::from(byte - b'0'))
}

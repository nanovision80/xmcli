// Constantes do formato S3M (Scream Tracker 3), vindas da especificação.

pub const TITLE_OFFSET: usize = 0x00;
pub const TITLE_LEN: usize = 28;

/// Assinatura, a 0x2C — o S3M se identifica no meio do cabeçalho, não no início.
pub const MAGIC_OFFSET: usize = 0x2C;
pub const MAGIC: &[u8] = b"SCRM";

pub const ORDER_COUNT_OFFSET: usize = 0x20;
pub const INSTRUMENT_COUNT_OFFSET: usize = 0x22;
pub const PATTERN_COUNT_OFFSET: usize = 0x24;
/// Versão do programa que gravou: nibble alto identifica o tracker, resto é a versão.
pub const TRACKER_VERSION_OFFSET: usize = 0x28;

/// Painel de canais: 32 bytes, um por canal.
pub const CHANNEL_TABLE_OFFSET: usize = 0x40;
pub const CHANNEL_TABLE_LEN: usize = 32;
/// Canal sem uso.
pub const CHANNEL_UNUSED: u8 = 0xFF;
/// Bit que marca o canal como desligado.
pub const CHANNEL_DISABLED: u8 = 0x80;

pub const ORDER_TABLE_OFFSET: usize = 0x60;

/// Os ponteiros do S3M são "parágrafos" de 16 bytes.
pub const PARAGRAPH: usize = 16;
/// Título da amostra dentro do cabeçalho de instrumento.
pub const INSTRUMENT_NAME_OFFSET: usize = 0x30;
pub const INSTRUMENT_NAME_LEN: usize = 28;

/// Trackers que gravam S3M, pelo nibble alto do campo de versão.
const TRACKERS: [(u16, &str); 7] = [
    (1, "Scream Tracker"),
    (2, "Imago Orpheus"),
    (3, "Impulse Tracker"),
    (4, "Schism Tracker"),
    (5, "OpenMPT"),
    (6, "BeRoTracker"),
    (7, "CreamTracker"),
];

/// Descreve o programa que gravou o arquivo, no formato "Nome 3.20".
pub fn tracker_name(version: u16) -> Option<String> {
    const NIBBLE: u32 = 4;
    const BYTE: u32 = 8;
    let id = version >> (NIBBLE * 3);
    let name = TRACKERS
        .iter()
        .find(|(code, _)| *code == id)
        .map(|(_, name)| *name)?;
    let major = (version >> BYTE) & 0x0F;
    let minor = version & 0xFF;
    Some(format!("{name} {major}.{minor:02X}"))
}

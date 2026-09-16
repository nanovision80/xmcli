// Constantes do formato IT (Impulse Tracker), vindas da especificação.

pub const MAGIC: &[u8] = b"IMPM";
pub const MAGIC_OFFSET: usize = 0x00;

pub const TITLE_OFFSET: usize = 0x04;
pub const TITLE_LEN: usize = 26;

pub const ORDER_COUNT_OFFSET: usize = 0x20;
pub const INSTRUMENT_COUNT_OFFSET: usize = 0x22;
pub const SAMPLE_COUNT_OFFSET: usize = 0x24;
pub const PATTERN_COUNT_OFFSET: usize = 0x26;
pub const TRACKER_VERSION_OFFSET: usize = 0x28;
/// Bit de `Flags` que diz se o módulo está em modo instrumento (e não em modo amostra).
pub const FLAGS_OFFSET: usize = 0x2C;
pub const FLAG_INSTRUMENT_MODE: u16 = 0x0004;

pub const MESSAGE_LEN_OFFSET: usize = 0x36;
pub const MESSAGE_PTR_OFFSET: usize = 0x38;

/// Painel de canais: 64 posições de panorama.
pub const CHANNEL_PAN_OFFSET: usize = 0x40;
pub const CHANNEL_COUNT: usize = 64;
/// Valor de panorama a partir do qual o canal está desligado.
pub const CHANNEL_DISABLED: u8 = 128;

/// Tabela de ordens, logo após o painel de volume de canal.
pub const ORDER_TABLE_OFFSET: usize = 0xC0;

/// Nome dentro do cabeçalho de instrumento e de amostra.
pub const INSTRUMENT_NAME_OFFSET: usize = 0x20;
pub const SAMPLE_NAME_OFFSET: usize = 0x14;
pub const NAME_LEN: usize = 26;

/// Trackers que gravam IT, pelo nibble alto do campo de versão.
const TRACKERS: [(u16, &str); 4] = [
    (0, "Impulse Tracker"),
    (1, "Schism Tracker"),
    (2, "OpenMPT"),
    (4, "pyIT"),
];

/// Descreve o programa que gravou o arquivo, no formato "Nome 2.14".
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

// Constantes do formato XM (FastTracker II), vindas da especificação.

pub const MAGIC: &[u8] = b"Extended Module: ";
pub const MAGIC_OFFSET: usize = 0x00;

pub const TITLE_OFFSET: usize = 0x11;
pub const TITLE_LEN: usize = 20;
pub const TRACKER_OFFSET: usize = 0x26;
pub const TRACKER_LEN: usize = 20;

/// A partir daqui o cabeçalho tem tamanho declarado, e os campos são relativos a ele.
pub const HEADER_SIZE_OFFSET: usize = 0x3C;
pub const ORDER_COUNT_OFFSET: usize = 0x40;
pub const CHANNEL_COUNT_OFFSET: usize = 0x44;
pub const PATTERN_COUNT_OFFSET: usize = 0x46;
pub const INSTRUMENT_COUNT_OFFSET: usize = 0x48;

/// Cabeçalho de padrão: tamanho (u32), empacotamento (u8), linhas (u16), dados (u16).
pub const PATTERN_PACKED_SIZE_OFFSET: usize = 7;

/// Cabeçalho de instrumento: tamanho (u32), nome (22), tipo (u8), nº de amostras (u16).
pub const INSTRUMENT_NAME_OFFSET: usize = 4;
pub const INSTRUMENT_NAME_LEN: usize = 22;
pub const INSTRUMENT_SAMPLE_COUNT_OFFSET: usize = 27;
pub const INSTRUMENT_SAMPLE_HEADER_SIZE_OFFSET: usize = 29;

/// Cabeçalho de amostra: comprimento (u32) no início, nome (22) a 0x12.
pub const SAMPLE_NAME_OFFSET: usize = 0x12;
pub const SAMPLE_NAME_LEN: usize = 22;

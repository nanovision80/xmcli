//! Leitura dos metadados de módulos S3M (RF-120).

mod spec;

use crate::formats::reader::{Reader, Truncated};
use crate::formats::text;
use crate::song::{Dialect, Song};

pub fn detect(reader: &Reader<'_>) -> bool {
    reader.has_magic(spec::MAGIC_OFFSET, spec::MAGIC)
}

pub fn read(reader: &Reader<'_>) -> Result<Song, Truncated> {
    let title = text::decode_name(reader.slice(spec::TITLE_OFFSET, spec::TITLE_LEN, "título")?);
    let orders = reader.u16_le(spec::ORDER_COUNT_OFFSET, "número de ordens")?;
    let instruments = reader.u16_le(spec::INSTRUMENT_COUNT_OFFSET, "número de instrumentos")?;
    let patterns = reader.u16_le(spec::PATTERN_COUNT_OFFSET, "número de padrões")?;
    let version = reader.u16_le(spec::TRACKER_VERSION_OFFSET, "versão do tracker")?;

    let channels = reader
        .slice(
            spec::CHANNEL_TABLE_OFFSET,
            spec::CHANNEL_TABLE_LEN,
            "painel de canais",
        )?
        .iter()
        .filter(|setting| {
            **setting != spec::CHANNEL_UNUSED && **setting & spec::CHANNEL_DISABLED == 0
        })
        .count();

    Ok(Song {
        dialect: Dialect::ScreamTracker3,
        title,
        tracker: spec::tracker_name(version),
        channels: u16::try_from(channels).unwrap_or(u16::MAX),
        orders,
        patterns,
        instruments: Vec::new(),
        samples: sample_names(reader, orders, instruments)?,
        message: None,
    })
}

/// Lê os nomes das amostras seguindo a tabela de ponteiros que vem após as ordens.
fn sample_names(
    reader: &Reader<'_>,
    orders: u16,
    instruments: u16,
) -> Result<Vec<String>, Truncated> {
    let table = spec::ORDER_TABLE_OFFSET + usize::from(orders);
    (0..usize::from(instruments))
        .map(|index| {
            let pointer = reader.u16_le(table + index * 2, "ponteiro de instrumento")?;
            let header = usize::from(pointer) * spec::PARAGRAPH;
            let raw = reader.slice(
                header + spec::INSTRUMENT_NAME_OFFSET,
                spec::INSTRUMENT_NAME_LEN,
                "nome de amostra",
            )?;
            Ok(text::decode_name(raw))
        })
        .collect()
}

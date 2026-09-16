//! Leitura dos metadados de módulos IT (RF-140).

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
    let instrument_count =
        reader.u16_le(spec::INSTRUMENT_COUNT_OFFSET, "número de instrumentos")?;
    let sample_count = reader.u16_le(spec::SAMPLE_COUNT_OFFSET, "número de amostras")?;
    let patterns = reader.u16_le(spec::PATTERN_COUNT_OFFSET, "número de padrões")?;
    let version = reader.u16_le(spec::TRACKER_VERSION_OFFSET, "versão do tracker")?;
    let flags = reader.u16_le(spec::FLAGS_OFFSET, "flags")?;

    // O IT não declara quantos canais a música usa; o painel só diz quais estão ligados.
    // A contagem exata sai da varredura dos padrões, que entra na etapa 8.
    let channels = reader
        .slice(
            spec::CHANNEL_PAN_OFFSET,
            spec::CHANNEL_COUNT,
            "painel de canais",
        )?
        .iter()
        .filter(|pan| **pan < spec::CHANNEL_DISABLED)
        .count();

    // As tabelas de ponteiros vêm em sequência depois das ordens.
    let instrument_table = spec::ORDER_TABLE_OFFSET + usize::from(orders);
    let sample_table = instrument_table + usize::from(instrument_count) * 4;

    let instruments = if flags & spec::FLAG_INSTRUMENT_MODE == 0 {
        // Em modo amostra os cabeçalhos de instrumento podem existir e não significar nada.
        Vec::new()
    } else {
        names(
            reader,
            instrument_table,
            instrument_count,
            spec::INSTRUMENT_NAME_OFFSET,
        )?
    };

    Ok(Song {
        dialect: Dialect::ImpulseTracker,
        title,
        tracker: spec::tracker_name(version),
        channels: u16::try_from(channels).unwrap_or(u16::MAX),
        orders,
        patterns,
        instruments,
        samples: names(reader, sample_table, sample_count, spec::SAMPLE_NAME_OFFSET)?,
        message: message(reader)?,
    })
}

fn names(
    reader: &Reader<'_>,
    table: usize,
    count: u16,
    name_offset: usize,
) -> Result<Vec<String>, Truncated> {
    (0..usize::from(count))
        .map(|index| {
            let pointer = reader.u32_le(table + index * 4, "ponteiro de cabeçalho")? as usize;
            let raw = reader.slice(pointer + name_offset, spec::NAME_LEN, "nome")?;
            Ok(text::decode_name(raw))
        })
        .collect()
}

fn message(reader: &Reader<'_>) -> Result<Option<String>, Truncated> {
    let length = reader.u16_le(spec::MESSAGE_LEN_OFFSET, "tamanho da mensagem")?;
    let pointer = reader.u32_le(spec::MESSAGE_PTR_OFFSET, "ponteiro da mensagem")?;
    if length == 0 || pointer == 0 {
        return Ok(None);
    }
    let raw = reader.slice(pointer as usize, usize::from(length), "mensagem")?;
    let text = text::decode_message(raw);
    Ok((!text.is_empty()).then_some(text))
}

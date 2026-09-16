//! Leitura dos metadados de módulos XM (RF-130).

mod spec;

use crate::formats::reader::{Reader, Truncated};
use crate::formats::text;
use crate::song::{Dialect, Song};

pub fn detect(reader: &Reader<'_>) -> bool {
    reader.has_magic(spec::MAGIC_OFFSET, spec::MAGIC)
}

pub fn read(reader: &Reader<'_>) -> Result<Song, Truncated> {
    let title = text::decode_name(reader.slice(spec::TITLE_OFFSET, spec::TITLE_LEN, "título")?);
    let tracker =
        text::decode_name(reader.slice(spec::TRACKER_OFFSET, spec::TRACKER_LEN, "tracker")?);
    let orders = reader.u16_le(spec::ORDER_COUNT_OFFSET, "número de ordens")?;
    let channels = reader.u16_le(spec::CHANNEL_COUNT_OFFSET, "número de canais")?;
    let patterns = reader.u16_le(spec::PATTERN_COUNT_OFFSET, "número de padrões")?;
    let instrument_count =
        reader.u16_le(spec::INSTRUMENT_COUNT_OFFSET, "número de instrumentos")?;

    let after_patterns = skip_patterns(reader, patterns)?;
    let (instruments, samples) = read_instruments(reader, after_patterns, instrument_count)?;

    Ok(Song {
        dialect: Dialect::FastTracker2,
        title,
        tracker: (!tracker.is_empty()).then_some(tracker),
        channels,
        orders,
        patterns,
        instruments,
        samples,
        message: None,
    })
}

/// Percorre os padrões para chegar aos instrumentos.
///
/// O XM não guarda o deslocamento dos instrumentos: é preciso somar padrão a padrão. Cada
/// cabeçalho declara o próprio tamanho, então um tamanho zero significa arquivo corrompido —
/// e sem essa verificação o laço nunca terminaria.
fn skip_patterns(reader: &Reader<'_>, patterns: u16) -> Result<usize, Truncated> {
    let header_size = reader.u32_le(spec::HEADER_SIZE_OFFSET, "tamanho do cabeçalho")?;
    let mut offset = spec::HEADER_SIZE_OFFSET + header_size as usize;

    for _ in 0..patterns {
        let header = reader.u32_le(offset, "cabeçalho de padrão")? as usize;
        let packed = reader.u16_le(
            offset + spec::PATTERN_PACKED_SIZE_OFFSET,
            "tamanho do padrão",
        )? as usize;
        if header == 0 {
            return Err(truncated_at(
                "cabeçalho de padrão com tamanho zero",
                offset,
                reader,
            ));
        }
        offset = offset
            .checked_add(header + packed)
            .ok_or_else(|| truncated_at("padrão além do arquivo", offset, reader))?;
    }
    Ok(offset)
}

fn read_instruments(
    reader: &Reader<'_>,
    mut offset: usize,
    count: u16,
) -> Result<(Vec<String>, Vec<String>), Truncated> {
    let mut instruments = Vec::with_capacity(usize::from(count));
    let mut samples = Vec::new();

    for _ in 0..count {
        let size = reader.u32_le(offset, "cabeçalho de instrumento")? as usize;
        if size == 0 {
            return Err(truncated_at("instrumento com tamanho zero", offset, reader));
        }
        let name = reader.slice(
            offset + spec::INSTRUMENT_NAME_OFFSET,
            spec::INSTRUMENT_NAME_LEN,
            "nome de instrumento",
        )?;
        instruments.push(text::decode_name(name));

        let sample_count = reader.u16_le(
            offset + spec::INSTRUMENT_SAMPLE_COUNT_OFFSET,
            "número de amostras",
        )?;
        offset += size;

        if sample_count == 0 {
            continue;
        }
        let header_size = reader.u32_le(
            offset - size + spec::INSTRUMENT_SAMPLE_HEADER_SIZE_OFFSET,
            "tamanho do cabeçalho de amostra",
        )? as usize;
        if header_size == 0 {
            return Err(truncated_at(
                "cabeçalho de amostra com tamanho zero",
                offset,
                reader,
            ));
        }

        let mut sample_bytes = 0_usize;
        for index in 0..usize::from(sample_count) {
            let header = offset + index * header_size;
            sample_bytes += reader.u32_le(header, "comprimento de amostra")? as usize;
            let name = reader.slice(
                header + spec::SAMPLE_NAME_OFFSET,
                spec::SAMPLE_NAME_LEN,
                "nome de amostra",
            )?;
            samples.push(text::decode_name(name));
        }
        offset = offset
            .checked_add(usize::from(sample_count) * header_size + sample_bytes)
            .ok_or_else(|| truncated_at("amostra além do arquivo", offset, reader))?;
    }
    Ok((instruments, samples))
}

fn truncated_at(what: &'static str, offset: usize, reader: &Reader<'_>) -> Truncated {
    Truncated {
        what,
        offset,
        need: 1,
        len: reader.len(),
    }
}

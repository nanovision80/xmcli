//! Leitura dos metadados de módulos MOD (RF-110).

mod spec;

use crate::formats::reader::{Reader, Truncated};
use crate::formats::text;
use crate::song::{Dialect, Song};

/// Variante do formato encontrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    /// 31 amostras e assinatura de quatro letras.
    Tagged { channels: u16 },
    /// 15 amostras, sem assinatura: o formato original do Ultimate Soundtracker.
    Tagless,
}

impl Variant {
    fn samples(self) -> usize {
        match self {
            Self::Tagged { .. } => spec::SAMPLES_TAGGED,
            Self::Tagless => spec::SAMPLES_TAGLESS,
        }
    }

    fn channels(self) -> u16 {
        match self {
            Self::Tagged { channels } => channels,
            Self::Tagless => spec::TAGLESS_CHANNELS,
        }
    }
}

/// Verdadeiro quando os bytes parecem um MOD.
pub fn detect(reader: &Reader<'_>) -> bool {
    variant(reader).is_some()
}

/// Lê os metadados do módulo.
pub fn read(reader: &Reader<'_>) -> Result<Song, Truncated> {
    let variant = variant(reader).ok_or(Truncated {
        what: "assinatura de MOD",
        offset: spec::tag_offset(),
        need: spec::TAG_LEN,
        len: reader.len(),
    })?;

    let title = text::decode_name(reader.slice(spec::TITLE_OFFSET, spec::TITLE_LEN, "título")?);
    let samples = sample_names(reader, variant.samples())?;
    let orders = reader.u8(spec::song_length_offset(variant.samples()), "comprimento")?;
    let table = reader.slice(
        spec::order_table_offset(variant.samples()),
        spec::ORDER_TABLE_LEN,
        "tabela de ordens",
    )?;

    // O MOD não guarda a contagem de padrões: ela é a maior ordem usada, mais um.
    let patterns = table
        .iter()
        .copied()
        .max()
        .map_or(0, |highest| u16::from(highest) + 1);

    Ok(Song {
        dialect: Dialect::ProTracker,
        title,
        tracker: None,
        channels: variant.channels(),
        orders: u16::from(orders),
        patterns,
        instruments: Vec::new(),
        samples,
        message: None,
    })
}

fn sample_names(reader: &Reader<'_>, count: usize) -> Result<Vec<String>, Truncated> {
    (0..count)
        .map(|index| {
            let offset = spec::SAMPLE_TABLE_OFFSET + index * spec::SAMPLE_ENTRY_LEN;
            let raw = reader.slice(offset, spec::SAMPLE_NAME_LEN, "nome de amostra")?;
            Ok(text::decode_name(raw))
        })
        .collect()
}

fn variant(reader: &Reader<'_>) -> Option<Variant> {
    let tag = reader
        .slice(spec::tag_offset(), spec::TAG_LEN, "assinatura")
        .ok();
    if let Some(channels) = tag.and_then(spec::channels_from_tag) {
        return Some(Variant::Tagged { channels });
    }
    looks_tagless(reader).then_some(Variant::Tagless)
}

/// Heurística para a variante sem assinatura.
///
/// Sem assinatura não há como ter certeza, então três sinais precisam concordar: sequência de
/// comprimento plausível, ordens dentro da faixa válida e amostras que caibam no arquivo.
/// Errar para o lado de recusar é melhor que tocar ruído a partir de um arquivo qualquer.
fn looks_tagless(reader: &Reader<'_>) -> bool {
    let samples = spec::SAMPLES_TAGLESS;
    let Ok(orders) = reader.u8(spec::song_length_offset(samples), "comprimento") else {
        return false;
    };
    if orders == 0 || usize::from(orders) > spec::ORDER_TABLE_LEN {
        return false;
    }

    let Ok(table) = reader.slice(
        spec::order_table_offset(samples),
        spec::ORDER_TABLE_LEN,
        "tabela de ordens",
    ) else {
        return false;
    };
    if table
        .iter()
        .any(|order| *order >= spec::ORDER_TABLE_LEN as u8)
    {
        return false;
    }

    let mut total_sample_bytes: usize = 0;
    for index in 0..samples {
        let offset =
            spec::SAMPLE_TABLE_OFFSET + index * spec::SAMPLE_ENTRY_LEN + spec::SAMPLE_LENGTH_OFFSET;
        let Ok(words) = reader.u16_be(offset, "comprimento de amostra") else {
            return false;
        };
        total_sample_bytes += usize::from(words) * 2;
    }

    total_sample_bytes > 0 && total_sample_bytes <= reader.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assinaturas_conhecidas_dao_o_numero_de_canais() {
        assert_eq!(spec::channels_from_tag(b"M.K."), Some(4));
        assert_eq!(spec::channels_from_tag(b"6CHN"), Some(6));
        assert_eq!(spec::channels_from_tag(b"FLT8"), Some(8));
        assert_eq!(spec::channels_from_tag(b"16CH"), Some(16));
        assert_eq!(spec::channels_from_tag(b"32CH"), Some(32));
        assert_eq!(spec::channels_from_tag(b"TDZ3"), Some(3));
        assert_eq!(spec::channels_from_tag(b"XXXX"), None);
    }
}

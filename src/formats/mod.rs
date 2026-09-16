//! Detecção de formato e leitura de metadados (RF-101, RF-102).
//!
//! O formato é reconhecido **pelo conteúdo**, nunca pela extensão: coleções antigas estão
//! cheias de `.mod` que são XM e de arquivos sem extensão nenhuma.

mod fasttracker;
mod impulsetracker;
mod protracker;
pub mod reader;
mod screamtracker;
pub mod text;

use thiserror::Error;

use crate::song::{Dialect, Song};
use reader::{Reader, Truncated};

/// Falha ao interpretar o arquivo de entrada.
#[derive(Debug, Error)]
pub enum FormatError {
    #[error("formato não reconhecido: não parece um módulo .mod, .s3m, .xm ou .it")]
    Unsupported,

    #[error("módulo {} malformado", dialect.tracker_name())]
    Malformed {
        dialect: Dialect,
        #[source]
        source: Truncated,
    },
}

/// Descobre o dialeto a partir dos bytes do arquivo.
///
/// A ordem importa: as assinaturas fortes (IT, S3M, XM) vêm antes do MOD, cuja variante sem
/// assinatura depende de heurística e aceitaria arquivos que não são dela.
pub fn detect(bytes: &[u8]) -> Option<Dialect> {
    let reader = Reader::new(bytes);
    if impulsetracker::detect(&reader) {
        return Some(Dialect::ImpulseTracker);
    }
    if screamtracker::detect(&reader) {
        return Some(Dialect::ScreamTracker3);
    }
    if fasttracker::detect(&reader) {
        return Some(Dialect::FastTracker2);
    }
    protracker::detect(&reader).then_some(Dialect::ProTracker)
}

/// Lê os metadados do módulo.
pub fn read(bytes: &[u8]) -> Result<Song, FormatError> {
    let dialect = detect(bytes).ok_or(FormatError::Unsupported)?;
    let reader = Reader::new(bytes);
    let song = match dialect {
        Dialect::ProTracker => protracker::read(&reader),
        Dialect::ScreamTracker3 => screamtracker::read(&reader),
        Dialect::FastTracker2 => fasttracker::read(&reader),
        Dialect::ImpulseTracker => impulsetracker::read(&reader),
    };
    song.map_err(|source| FormatError::Malformed { dialect, source })
}

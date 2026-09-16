//! Modelo intermediário de uma música e o dialeto de tracker que a rege.
//!
//! O modelo é comum aos quatro formatos, mas o [`Dialect`] viaja junto até o replayer: os
//! *quirks* de cada tracker são comportamento correto, e normalizá-los é a causa nº 1 de
//! reprodução errada (CLAUDE.md §2, invariante 5).
//!
//! Nesta etapa o modelo carrega apenas os metadados que `--info` mostra; padrões, amostras e
//! eventos entram na etapa 8.

use serde::Serialize;

/// Tracker que definiu o comportamento da música.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dialect {
    /// `.mod` — ProTracker e derivados do Amiga.
    ProTracker,
    /// `.s3m` — Scream Tracker 3.
    ScreamTracker3,
    /// `.xm` — FastTracker II.
    FastTracker2,
    /// `.it` — Impulse Tracker.
    ImpulseTracker,
}

impl Dialect {
    /// Extensão canônica do formato.
    pub fn extension(self) -> &'static str {
        match self {
            Self::ProTracker => "mod",
            Self::ScreamTracker3 => "s3m",
            Self::FastTracker2 => "xm",
            Self::ImpulseTracker => "it",
        }
    }

    /// Nome do tracker de origem, para exibição.
    pub fn tracker_name(self) -> &'static str {
        match self {
            Self::ProTracker => "ProTracker",
            Self::ScreamTracker3 => "Scream Tracker 3",
            Self::FastTracker2 => "FastTracker II",
            Self::ImpulseTracker => "Impulse Tracker",
        }
    }
}

/// Metadados de uma música carregada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Song {
    pub dialect: Dialect,
    /// Título declarado no módulo, já convertido de CP437 para UTF-8.
    pub title: String,
    /// Programa que gravou o arquivo, quando o formato registra.
    pub tracker: Option<String>,
    /// Canais em uso; para IT é uma estimativa a partir do painel de canais.
    pub channels: u16,
    /// Posições na sequência de reprodução.
    pub orders: u16,
    pub patterns: u16,
    /// Nomes dos instrumentos, para os formatos que os têm.
    pub instruments: Vec<String>,
    /// Nomes das amostras.
    pub samples: Vec<String>,
    /// Mensagem embutida pelo autor, quando existe.
    pub message: Option<String>,
}

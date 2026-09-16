//! Relatório de metadados de um módulo, em texto e em JSON (RF-703).

use std::fmt::Write as _;

use serde::Serialize;

use crate::song::Song;

/// Um módulo e sua origem, prontos para exibição.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Origem do módulo, como o usuário a informou.
    pub file: String,
    pub song: Song,
}

/// Largura da coluna de rótulos no relatório em texto.
const LABEL_WIDTH: usize = 13;

/// Quantos nomes de amostra ou instrumento listar antes de resumir.
const NAME_LIST_LIMIT: usize = 64;

impl Report {
    /// Relatório legível para o terminal.
    pub fn to_text(&self) -> String {
        let song = &self.song;
        let mut out = format!("{}\n", self.file);

        field(
            &mut out,
            "formato",
            &format!(".{}", song.dialect.extension()),
        );
        field(&mut out, "dialeto", song.dialect.tracker_name());
        if let Some(tracker) = &song.tracker {
            field(&mut out, "gravado por", tracker);
        }
        field(&mut out, "título", &quoted(&song.title));
        field(&mut out, "canais", &song.channels.to_string());
        field(&mut out, "ordens", &song.orders.to_string());
        field(&mut out, "padrões", &song.patterns.to_string());
        if !song.instruments.is_empty() {
            field(
                &mut out,
                "instrumentos",
                &song.instruments.len().to_string(),
            );
        }
        field(&mut out, "amostras", &song.samples.len().to_string());

        name_list(&mut out, "instrumentos", &song.instruments);
        name_list(&mut out, "amostras", &song.samples);

        if let Some(message) = &song.message {
            out.push_str("\n  mensagem\n");
            for line in message.lines() {
                let _ = writeln!(out, "    {line}");
            }
        }
        out
    }

    /// Mesmo conteúdo em JSON, para scripts.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn field(out: &mut String, label: &str, value: &str) {
    let _ = writeln!(out, "  {label:<LABEL_WIDTH$}{value}");
}

/// Lista nomes numerados, pulando os vazios — módulos costumam declarar mais do que usam.
fn name_list(out: &mut String, title: &str, names: &[String]) {
    let used: Vec<(usize, &String)> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .collect();
    if used.is_empty() {
        return;
    }

    let _ = writeln!(out, "\n  {title}");
    for (index, name) in used.iter().take(NAME_LIST_LIMIT) {
        let _ = writeln!(out, "    {:>3}  {name}", index + 1);
    }
    if used.len() > NAME_LIST_LIMIT {
        let _ = writeln!(out, "    ... e mais {}", used.len() - NAME_LIST_LIMIT);
    }
}

fn quoted(text: &str) -> String {
    if text.is_empty() {
        return "(sem título)".to_owned();
    }
    format!("\"{text}\"")
}

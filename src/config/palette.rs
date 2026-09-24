// Temas de cor (RF-517).
//
// Este arquivo é a DEFINIÇÃO ÚNICA do formato de `config/palettes.json`. Como `settings.rs`, é
// incluído por `build.rs` para que um tema inválido quebre a compilação; por isso só depende
// de `serde` e da biblioteca padrão, e os comentários de topo são `//`. A documentação do
// módulo está na declaração `mod palette` em `mod.rs`.
//
// O arquivo é um objeto de nome para tema. Cada cor é `#rrggbb`; cada campo entra com o
// primeiro elemento da interface que o pinta.
use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

/// Prefixo de uma cor em hexadecimal.
const HEX_PREFIX: char = '#';

/// Dígitos hexadecimais de uma cor: dois por canal.
const HEX_DIGITS: usize = 6;

/// Base da notação hexadecimal.
const HEX_RADIX: u32 = 16;

/// Uma cor de tema, em 24 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_hex(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("cor \"{text}\" não está no formato #rrggbb"))
        })
    }
}

fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix(HEX_PREFIX)?;
    if digits.len() != HEX_DIGITS || !digits.is_ascii() {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], HEX_RADIX).ok();
    Some(Color {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
    })
}

/// As cores de um tema.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    /// Fundo de toda a tela: cromo e palco.
    pub background: Color,
    /// Moldura que separa os painéis.
    pub frame: Color,
    /// Texto corrido do cromo.
    pub text: Color,
    /// O que mostra o estado atual, como o botão ativo do transporte.
    pub active: Color,
}

/// Todos os temas, por nome.
pub type Palettes = HashMap<String, Theme>;

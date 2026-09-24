// Teclas -> ações.
//
// Este arquivo é a DEFINIÇÃO ÚNICA do formato de `config/keymap.json`. Como `settings.rs`, é
// incluído por `build.rs` (via `include!`) para que um atalho inválido quebre a compilação;
// por isso só depende de `serde`, `thiserror` e da biblioteca padrão, e os comentários de topo
// são `//`. A documentação do módulo está na declaração `mod keymap` em `mod.rs`.
//
// O arquivo é um objeto de tecla para ação: `{ "q": "quit", "ctrl+c": "quit" }`. A tecla é
// uma letra ou um nome (`space`, `left`, `f1`...), com `ctrl+` ou `alt+` na frente. Letra é
// sensível a maiúscula: `q` e `Q` são teclas diferentes, como no teclado.
use std::collections::HashMap;

use serde::Deserialize;
use thiserror::Error;

/// Separador entre modificador e tecla: `ctrl+c`.
const MODIFIER_SEPARATOR: char = '+';

const CTRL: &str = "ctrl";
const ALT: &str = "alt";

/// Nomes das teclas que não são um caractere impresso.
const NAMED_KEYS: [(&str, KeyCode); 13] = [
    ("space", KeyCode::Char(' ')),
    ("enter", KeyCode::Enter),
    ("tab", KeyCode::Tab),
    ("esc", KeyCode::Esc),
    ("backspace", KeyCode::Backspace),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
];

/// Prefixo das teclas de função: `f1` a `f12`.
const FUNCTION_PREFIX: char = 'f';

/// Maior tecla de função que os terminais enviam de forma confiável.
const FUNCTION_MAX: u8 = 12;

/// O que uma tecla pode pedir. Cada ação entra com a função que a atende.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Sair do programa.
    Quit,
    /// Tocar; com a música tocando, recomeçar a faixa (RF-503).
    Play,
    /// Pausar e retomar; parado, não faz nada.
    Pause,
    /// Alternar entre tocar e pausar, de qualquer estado.
    PlayPause,
    /// Parar e voltar ao começo da faixa.
    Stop,
    /// Faixa anterior da lista.
    Previous,
    /// Próxima faixa da lista.
    Next,
}

/// Uma tecla, independente de biblioteca de terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    Esc,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// Uma tecla com os modificadores que contam.
///
/// Shift não é modificador aqui: numa letra ele já está no caractere (`Q`), e nas teclas de
/// nome os terminais não o enviam de forma consistente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

/// Atalho que não dá para entender.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeymapError {
    #[error("tecla \"{0}\" não reconhecida")]
    UnknownKey(String),

    #[error("\"{first}\" e \"{second}\" são a mesma tecla")]
    Duplicate { first: String, second: String },
}

/// Teclas ligadas a ações, resolvidas uma vez na partida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: HashMap<Key, Action>,
}

impl Keymap {
    /// Interpreta o objeto de tecla para ação lido do JSON.
    pub fn from_names(names: HashMap<String, Action>) -> Result<Self, KeymapError> {
        let mut bindings = HashMap::new();
        let mut spelled: HashMap<Key, String> = HashMap::new();
        for (name, action) in names {
            let key = parse_key(&name)?;
            if let Some(first) = spelled.insert(key, name.clone()) {
                return Err(KeymapError::Duplicate {
                    first,
                    second: name,
                });
            }
            bindings.insert(key, action);
        }
        Ok(Self { bindings })
    }

    /// A ação ligada a esta tecla, se houver.
    pub fn action(&self, key: Key) -> Option<Action> {
        self.bindings.get(&key).copied()
    }
}

/// Lê uma tecla escrita como no `keymap.json`.
pub fn parse_key(name: &str) -> Result<Key, KeymapError> {
    let unknown = || KeymapError::UnknownKey(name.to_owned());
    let (mut ctrl, mut alt) = (false, false);

    let mut rest = name;
    // Tecla vazia depois do `+` quer dizer que a tecla é o próprio `+`: `ctrl++`.
    while let Some((modifier, tail)) = rest.split_once(MODIFIER_SEPARATOR) {
        if tail.is_empty() {
            break;
        }
        match modifier {
            CTRL => ctrl = true,
            ALT => alt = true,
            _ => return Err(unknown()),
        }
        rest = tail;
    }

    let code = key_code(rest).ok_or_else(unknown)?;
    Ok(Key { code, ctrl, alt })
}

fn key_code(name: &str) -> Option<KeyCode> {
    let mut chars = name.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return (!ch.is_control() && !ch.is_whitespace()).then_some(KeyCode::Char(ch));
    }
    if let Some((_, code)) = NAMED_KEYS.iter().find(|(known, _)| *known == name) {
        return Some(*code);
    }
    let number: u8 = name.strip_prefix(FUNCTION_PREFIX)?.parse().ok()?;
    (1..=FUNCTION_MAX)
        .contains(&number)
        .then_some(KeyCode::F(number))
}

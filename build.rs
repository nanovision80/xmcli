//! Valida `config/defaults.json` em tempo de compilação.
//!
//! A definição do formato vive em `src/config/settings.rs` e é incluída aqui em vez de
//! duplicada: um schema JSON separado seria o mesmo conhecimento em dois lugares
//! (CLAUDE.md §3, DRY). Um padrão inválido quebra o build, não a execução.

include!("src/config/settings.rs");

mod palette {
    include!("src/config/palette.rs");
}

// O build só valida o arquivo; consultar teclas é trabalho do programa.
#[allow(dead_code)]
mod keymap {
    include!("src/config/keymap.rs");
}

/// Padrões do projeto, a fonte única da verdade dos valores (CLAUDE.md §4).
const DEFAULTS_PATH: &str = "config/defaults.json";

/// Teclas padrão (etapa 5.8).
const KEYMAP_PATH: &str = "config/keymap.json";

/// Temas de cor (RF-517).
const PALETTES_PATH: &str = "config/palettes.json";

fn main() {
    println!("cargo::rerun-if-changed={DEFAULTS_PATH}");
    println!("cargo::rerun-if-changed=src/config/settings.rs");

    let raw = std::fs::read_to_string(DEFAULTS_PATH)
        .unwrap_or_else(|error| panic!("não foi possível ler {DEFAULTS_PATH}: {error}"));

    let settings: Settings = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("{DEFAULTS_PATH} não corresponde a Settings: {error}"));

    if let Err(error) = settings.validate() {
        panic!("{DEFAULTS_PATH} tem valor inválido: {error}");
    }

    println!("cargo::rerun-if-changed={KEYMAP_PATH}");
    println!("cargo::rerun-if-changed=src/config/keymap.rs");
    let raw = std::fs::read_to_string(KEYMAP_PATH)
        .unwrap_or_else(|error| panic!("não foi possível ler {KEYMAP_PATH}: {error}"));
    let names = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("{KEYMAP_PATH} não é um mapa de tecla para ação: {error}"));
    if let Err(error) = keymap::Keymap::from_names(names) {
        panic!("{KEYMAP_PATH} tem atalho inválido: {error}");
    }

    println!("cargo::rerun-if-changed={PALETTES_PATH}");
    println!("cargo::rerun-if-changed=src/config/palette.rs");
    let raw = std::fs::read_to_string(PALETTES_PATH)
        .unwrap_or_else(|error| panic!("não foi possível ler {PALETTES_PATH}: {error}"));
    let palettes: palette::Palettes = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("{PALETTES_PATH} não corresponde aos temas: {error}"));
    if !palettes.contains_key(&settings.ui.theme) {
        panic!(
            "{DEFAULTS_PATH} escolhe o tema \"{}\", que não existe em {PALETTES_PATH}",
            settings.ui.theme
        );
    }
}

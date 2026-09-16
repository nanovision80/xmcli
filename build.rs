//! Valida `config/defaults.json` em tempo de compilação.
//!
//! A definição do formato vive em `src/config/settings.rs` e é incluída aqui em vez de
//! duplicada: um schema JSON separado seria o mesmo conhecimento em dois lugares
//! (CLAUDE.md §3, DRY). Um padrão inválido quebra o build, não a execução.

include!("src/config/settings.rs");

/// Padrões do projeto, a fonte única da verdade dos valores (CLAUDE.md §4).
const DEFAULTS_PATH: &str = "config/defaults.json";

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
}

//! Carga, mesclagem e validação da configuração (RF-706).
//!
//! Precedência: **CLI > ambiente > usuário > padrão**. As camadas são mescladas como JSON e
//! só então convertidas para [`Settings`], de modo que a regra de mesclagem existe em um
//! único lugar e acrescentar um ajuste novo não exige tocar em cada camada.
//!
//! O núcleo aqui é puro: quem fala com o sistema de arquivos é [`read_user_file`], na borda.

/// Forma e validação da configuração; compartilhado com `build.rs`.
mod settings;

/// Teclas ligadas a ações (etapa 5.8); compartilhado com `build.rs`.
mod keymap;

pub use keymap::{Action, Key, KeyCode, Keymap, KeymapError, parse_key};

pub use settings::{LogLevel, OutOfRange, Settings, Ui};

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use thiserror::Error;

/// Padrões do projeto. Validados em tempo de compilação por `build.rs`.
const DEFAULTS_JSON: &str = include_str!("../../config/defaults.json");

/// Teclas padrão. Validadas em tempo de compilação por `build.rs`.
const KEYMAP_JSON: &str = include_str!("../../config/keymap.json");

/// Nome da aplicação, usado para localizar o diretório de configuração do usuário.
const APP_NAME: &str = "xmcli";

/// Arquivo de configuração dentro do diretório do usuário.
const USER_CONFIG_FILE: &str = "config.json";

/// Prefixo das variáveis de ambiente que sobrescrevem a configuração.
const ENV_PREFIX: &str = "XMCLI_";

/// Separador de seção nas variáveis de ambiente: `XMCLI_AUDIO__SAMPLE_RATE`.
///
/// Underscore duplo porque o underscore simples faz parte dos nomes de campo.
const ENV_SECTION_SEPARATOR: &str = "__";

/// Falha ao obter a configuração final.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("não foi possível ler {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON inválido em {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("configuração não corresponde ao formato esperado")]
    Shape(#[source] serde_json::Error),

    #[error(transparent)]
    OutOfRange(#[from] OutOfRange),
}

/// Lê o arquivo de configuração do usuário, se houver.
///
/// Um caminho passado explicitamente que não exista é erro — o usuário pediu por ele. O
/// arquivo descoberto pelo padrão do sistema, ao contrário, é opcional.
pub fn read_user_file(explicit: Option<&Path>) -> Result<Option<Value>, ConfigError> {
    let path = match explicit {
        Some(path) => path.to_path_buf(),
        None => match default_user_path() {
            Some(path) if path.is_file() => path,
            _ => return Ok(None),
        },
    };

    let raw = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let value = serde_json::from_str(&raw).map_err(|source| ConfigError::Parse { path, source })?;
    Ok(Some(value))
}

/// Caminho convencional do arquivo de configuração do usuário no sistema em uso.
pub fn default_user_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", APP_NAME)?;
    Some(dirs.config_dir().join(USER_CONFIG_FILE))
}

/// Mescla as camadas na ordem de precedência e devolve a configuração congelada.
pub fn resolve(
    user: Option<&Value>,
    env_vars: impl Iterator<Item = (String, String)>,
    cli: &Value,
) -> Result<Settings, ConfigError> {
    let mut merged: Value = serde_json::from_str(DEFAULTS_JSON)
        .expect("config/defaults.json é validado em tempo de compilação por build.rs");

    if let Some(user) = user {
        merge(&mut merged, user.clone());
    }
    merge(&mut merged, env_patch(env_vars));
    merge(&mut merged, cli.clone());

    let settings: Settings = serde_json::from_value(merged).map_err(ConfigError::Shape)?;
    settings.validate()?;
    Ok(settings)
}

/// As teclas padrão, resolvidas para consulta.
pub fn keymap() -> Keymap {
    let names = serde_json::from_str(KEYMAP_JSON)
        .expect("config/keymap.json é validado em tempo de compilação por build.rs");
    Keymap::from_names(names)
        .expect("config/keymap.json é validado em tempo de compilação por build.rs")
}

/// Sobrepõe `patch` a `base`, descendo em objetos e substituindo qualquer outro valor.
fn merge(base: &mut Value, patch: Value) {
    let Value::Object(patch_fields) = patch else {
        *base = patch;
        return;
    };
    for (key, value) in patch_fields {
        merge(object_at(base).entry(key).or_insert(Value::Null), value);
    }
}

/// Traduz as variáveis de ambiente do programa em uma camada de configuração.
///
/// O valor é interpretado como JSON quando possível (números e booleanos) e como texto caso
/// contrário, o que permite `XMCLI_AUDIO__SAMPLE_RATE=48000` e `XMCLI_LOG__LEVEL=debug` sem
/// uma tabela de conversão por campo. Caminho desconhecido é recusado adiante, na conversão
/// para [`Settings`].
fn env_patch(vars: impl Iterator<Item = (String, String)>) -> Value {
    let mut patch = Value::Object(Map::new());
    for (name, raw) in vars {
        let Some(path) = name
            .strip_prefix(ENV_PREFIX)
            .filter(|path| !path.is_empty())
        else {
            continue;
        };
        let segments: Vec<String> = path
            .split(ENV_SECTION_SEPARATOR)
            .map(str::to_ascii_lowercase)
            .collect();
        let value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
        set_path(&mut patch, &segments, value);
    }
    patch
}

/// Grava `value` em `target` no caminho dado, criando os objetos intermediários.
fn set_path(target: &mut Value, segments: &[String], value: Value) {
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut cursor = target;
    for segment in parents {
        cursor = object_at(cursor)
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    object_at(cursor).insert(last.clone(), value);
}

/// Garante que o nó é um objeto, substituindo qualquer outro valor que estivesse lá.
fn object_at(node: &mut Value) -> &mut Map<String, Value> {
    if !node.is_object() {
        *node = Value::Object(Map::new());
    }
    node.as_object_mut()
        .expect("o nó acabou de ser convertido em objeto acima")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env(vars: &[(&str, &str)]) -> impl Iterator<Item = (String, String)> + use<> {
        vars.iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn resolved(user: Option<Value>, vars: &[(&str, &str)], cli: Value) -> Settings {
        resolve(user.as_ref(), env(vars), &cli).expect("a configuração do teste é válida")
    }

    #[test]
    fn padroes_embutidos_sao_validos() {
        let settings = resolved(None, &[], json!({}));
        assert_eq!(settings.audio.sample_rate, 44_100);
        assert_eq!(settings.log.level, LogLevel::Warn);
    }

    #[test]
    fn usuario_sobrescreve_padrao() {
        let settings = resolved(
            Some(json!({"audio": {"sample_rate": 48_000}})),
            &[],
            json!({}),
        );
        assert_eq!(settings.audio.sample_rate, 48_000);
        // O campo vizinho, ausente na camada do usuário, mantém o padrão.
        assert_eq!(settings.audio.latency_ms, 30);
    }

    #[test]
    fn ambiente_sobrescreve_usuario() {
        let settings = resolved(
            Some(json!({"audio": {"sample_rate": 48_000}})),
            &[("XMCLI_AUDIO__SAMPLE_RATE", "96000")],
            json!({}),
        );
        assert_eq!(settings.audio.sample_rate, 96_000);
    }

    #[test]
    fn cli_vence_todas_as_camadas() {
        let settings = resolved(
            Some(json!({"audio": {"sample_rate": 48_000}})),
            &[("XMCLI_AUDIO__SAMPLE_RATE", "96000")],
            json!({"audio": {"sample_rate": 22_050}}),
        );
        assert_eq!(settings.audio.sample_rate, 22_050);
    }

    #[test]
    fn ambiente_aceita_numero_e_texto() {
        let settings = resolved(
            None,
            &[
                ("XMCLI_AUDIO__LATENCY_MS", "12"),
                ("XMCLI_LOG__LEVEL", "trace"),
                ("PATH", "isto não é configuração"),
            ],
            json!({}),
        );
        assert_eq!(settings.audio.latency_ms, 12);
        assert_eq!(settings.log.level, LogLevel::Trace);
    }

    #[test]
    fn chave_desconhecida_e_recusada() {
        let error = resolve(Some(&json!({"audio": {"volume": 5}})), env(&[]), &json!({}))
            .expect_err("chave inexistente deve ser recusada");
        assert!(matches!(error, ConfigError::Shape(_)), "obtido: {error}");
    }

    #[test]
    fn valor_fora_da_faixa_e_recusado() {
        let error = resolve(None, env(&[]), &json!({"audio": {"sample_rate": 8_000}}))
            .expect_err("taxa abaixo do mínimo deve ser recusada");
        let ConfigError::OutOfRange(out_of_range) = error else {
            panic!("esperado OutOfRange");
        };
        assert_eq!(out_of_range.field, "audio.sample_rate");
    }

    #[test]
    fn fps_minimo_acima_do_maximo_e_recusado() {
        let error = resolve(
            None,
            env(&[]),
            &json!({"ui": {"fps_max": 30, "fps_min": 60}}),
        )
        .expect_err("fps mínimo acima do máximo deve ser recusado");
        let ConfigError::OutOfRange(out_of_range) = error else {
            panic!("esperado OutOfRange");
        };
        assert_eq!(out_of_range.field, "ui.fps_min");
        assert_eq!(out_of_range.max, 30);
    }

    #[test]
    fn teclas_padrao_sao_validas_e_q_sai() {
        let key = parse_key("q").expect("q é tecla");
        assert_eq!(keymap().action(key), Some(Action::Quit));
    }

    #[test]
    fn tecla_com_modificadores_e_nomes() {
        let key = |code, ctrl, alt| Key { code, ctrl, alt };
        assert_eq!(
            parse_key("ctrl+c"),
            Ok(key(KeyCode::Char('c'), true, false))
        );
        assert_eq!(
            parse_key("alt+ctrl+left"),
            Ok(key(KeyCode::Left, true, true))
        );
        assert_eq!(
            parse_key("space"),
            Ok(key(KeyCode::Char(' '), false, false))
        );
        assert_eq!(parse_key("f12"), Ok(key(KeyCode::F(12), false, false)));
        assert_eq!(
            parse_key("ctrl++"),
            Ok(key(KeyCode::Char('+'), true, false))
        );
        assert_eq!(parse_key("Q"), Ok(key(KeyCode::Char('Q'), false, false)));
    }

    #[test]
    fn tecla_desconhecida_e_recusada() {
        for name in ["f13", "f0", "shift+a", "espaço", "", " ", "ctrl+"] {
            assert!(
                matches!(parse_key(name), Err(KeymapError::UnknownKey(_))),
                "{name:?} deveria ser recusada"
            );
        }
    }

    #[test]
    fn a_mesma_tecla_escrita_duas_vezes_e_recusada() {
        // A ordem dos modificadores não muda a tecla: um dos dois atalhos seria ignorado calado.
        let same = [("ctrl+alt+x", Action::Quit), ("alt+ctrl+x", Action::Quit)]
            .into_iter()
            .map(|(name, action)| (name.to_owned(), action))
            .collect();
        assert!(matches!(
            Keymap::from_names(same),
            Err(KeymapError::Duplicate { .. })
        ));
    }

    #[test]
    fn mesclagem_desce_em_objetos_e_substitui_o_resto() {
        let mut base = json!({"audio": {"sample_rate": 44_100, "latency_ms": 30}});
        merge(
            &mut base,
            json!({"audio": {"latency_ms": 10}, "log": {"level": "info"}}),
        );
        assert_eq!(
            base,
            json!({
                "audio": {"sample_rate": 44_100, "latency_ms": 10},
                "log": {"level": "info"}
            })
        );
    }
}

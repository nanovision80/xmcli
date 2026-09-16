//! Definição dos argumentos de linha de comando e sua tradução para configuração.

use std::path::PathBuf;

use clap::Parser;
use serde_json::{Value, json};

/// Códigos de saída do programa (RF-707).
///
/// O valor de [`ExitCode::Usage`] é o mesmo que o `clap` usa ao recusar argumentos, de modo
/// que um erro de uso tem sempre o mesmo código, venha de onde vier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Tudo correu bem.
    Success = 0,
    /// Falha durante a execução.
    Runtime = 1,
    /// Argumentos inválidos ou ausentes.
    Usage = 2,
    /// Arquivo de entrada em formato não suportado.
    Unsupported = 3,
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        Self::from(code as u8)
    }
}

/// Player de música tracker para o terminal.
#[derive(Debug, Parser)]
#[command(name = "xmcli", version, about, long_about = None)]
pub struct Args {
    /// Módulos ou diretórios a processar; "-" lê a entrada padrão
    #[arg(value_name = "ARQUIVO")]
    pub inputs: Vec<PathBuf>,

    /// Mostra os metadados dos módulos em vez de tocar
    #[arg(long, conflicts_with_all = ["render", "raw_stdout"])]
    pub info: bool,

    /// Emite o resultado de --info em JSON
    #[arg(long, requires = "info")]
    pub json: bool,

    /// Renderiza para um arquivo WAV em vez de tocar
    #[arg(long, value_name = "ARQUIVO", conflicts_with = "raw_stdout")]
    pub render: Option<PathBuf>,

    /// Escreve PCM cru (f32 little-endian, estéreo intercalado) na saída padrão
    #[arg(long = "stdout")]
    pub raw_stdout: bool,

    /// Nome (ou parte dele) do dispositivo de saída a usar
    #[arg(long, value_name = "NOME")]
    pub device: Option<String>,

    /// Usa este arquivo de configuração no lugar do arquivo do usuário
    #[arg(long, value_name = "ARQUIVO")]
    pub config: Option<PathBuf>,

    /// Taxa de amostragem da saída de áudio, em Hz
    #[arg(long, value_name = "HZ")]
    pub sample_rate: Option<u32>,

    /// Latência alvo do dispositivo de áudio, em milissegundos
    #[arg(long, value_name = "MS")]
    pub latency_ms: Option<u16>,

    /// Detalha o log em stderr (-v para debug, -vv para trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

/// Quantos `-v` elevam o log a `debug`; acima disso, `trace`.
const VERBOSE_DEBUG: u8 = 1;

impl Args {
    /// Converte os argumentos na camada de configuração de maior precedência.
    ///
    /// Só entra no resultado o que foi realmente informado: um argumento ausente não pode
    /// apagar o que veio do ambiente, do usuário ou do padrão.
    pub fn patch(&self) -> Value {
        let mut audio = json!({});
        if let Some(sample_rate) = self.sample_rate {
            audio["sample_rate"] = json!(sample_rate);
        }
        if let Some(latency_ms) = self.latency_ms {
            audio["latency_ms"] = json!(latency_ms);
        }

        let mut patch = json!({});
        if audio.as_object().is_some_and(|fields| !fields.is_empty()) {
            patch["audio"] = audio;
        }
        if self.verbose > 0 {
            let level = if self.verbose == VERBOSE_DEBUG {
                "debug"
            } else {
                "trace"
            };
            patch["log"] = json!({ "level": level });
        }
        patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Args {
        Args::try_parse_from(args).expect("argumentos do teste são válidos")
    }

    #[test]
    fn sem_argumentos_nao_gera_camada() {
        assert_eq!(parse(&["xmcli"]).patch(), json!({}));
    }

    #[test]
    fn argumentos_viram_caminhos_de_configuracao() {
        let patch = parse(&["xmcli", "--sample-rate", "48000", "-vv"]).patch();
        assert_eq!(
            patch,
            json!({"audio": {"sample_rate": 48_000}, "log": {"level": "trace"}})
        );
    }

    #[test]
    fn um_v_eleva_a_debug() {
        assert_eq!(
            parse(&["xmcli", "-v"]).patch()["log"]["level"],
            json!("debug")
        );
    }

    #[test]
    fn definicao_de_argumentos_e_coerente() {
        // Garante que nomes duplicados ou atributos inválidos falhem em teste, não no usuário.
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}

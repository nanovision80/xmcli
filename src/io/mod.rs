//! Entrada de dados: leitura com teto de tamanho e abertura de contêineres (RF-104, RF-105).
//!
//! Aqui mora a fronteira com o sistema de arquivos. O que sai daqui é um bloco de bytes na
//! memória, já desempacotado, que o resto do programa trata como **entrada hostil**.

pub mod container;

use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Caminho que representa a entrada padrão na linha de comando.
pub const STDIN_PATH: &str = "-";

/// Nome exibido para a entrada padrão em mensagens e relatórios.
const STDIN_NAME: &str = "<stdin>";

/// Um arquivo de entrada carregado na memória.
#[derive(Debug, Clone)]
pub struct Input {
    /// Nome legível da origem, para mensagens de erro e para o relatório.
    pub name: String,
    /// Conteúdo já desempacotado de qualquer contêiner.
    pub bytes: Vec<u8>,
}

/// Falha ao obter os bytes de entrada.
#[derive(Debug, Error)]
pub enum InputError {
    #[error("não foi possível ler {name}")]
    Read {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{name} tem mais de {limit} bytes; use io.max_file_bytes para elevar o teto")]
    TooLarge { name: String, limit: u64 },

    #[error("não foi possível abrir o contêiner de {name}")]
    Container {
        name: String,
        #[source]
        source: container::ContainerError,
    },
}

/// Lê a entrada indicada e desempacota os contêineres que encontrar.
///
/// `path` igual a [`STDIN_PATH`] lê a entrada padrão. O teto de tamanho vale tanto para o
/// arquivo lido quanto para o resultado de cada descompactação: sem ele, um cabeçalho
/// mentiroso vira exaustão de memória.
pub fn read(path: &Path, max_bytes: u64) -> Result<Input, InputError> {
    let name = display_name(path);
    let packed = if path == Path::new(STDIN_PATH) {
        read_capped(std::io::stdin().lock(), max_bytes, &name)?
    } else {
        let file = std::fs::File::open(path).map_err(|source| InputError::Read {
            name: name.clone(),
            source,
        })?;
        read_capped(file, max_bytes, &name)?
    };

    let bytes = container::unpack(packed, max_bytes).map_err(|source| InputError::Container {
        name: name.clone(),
        source,
    })?;
    Ok(Input { name, bytes })
}

/// Lista os arquivos a tocar a partir dos caminhos dados, descendo em diretórios (RF-701).
///
/// A ordem é estável para que a mesma linha de comando produza sempre a mesma sequência.
pub fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, InputError> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            collect_dir(path, &mut found)?;
        } else {
            found.push(path.clone());
        }
    }
    Ok(found)
}

fn collect_dir(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), InputError> {
    let name = display_name(dir);
    let entries = std::fs::read_dir(dir).map_err(|source| InputError::Read {
        name: name.clone(),
        source,
    })?;

    let mut level: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| InputError::Read {
            name: name.clone(),
            source,
        })?;
        level.push(entry.path());
    }
    level.sort();

    for path in level {
        if path.is_dir() {
            collect_dir(&path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

fn read_capped(source: impl Read, limit: u64, name: &str) -> Result<Vec<u8>, InputError> {
    let mut bytes = Vec::new();
    // Lê um byte além do teto para distinguir "exatamente no limite" de "estourou".
    let read = source
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| InputError::Read {
            name: name.to_owned(),
            source,
        })?;

    if read as u64 > limit {
        return Err(InputError::TooLarge {
            name: name.to_owned(),
            limit,
        });
    }
    Ok(bytes)
}

fn display_name(path: &Path) -> String {
    if path == Path::new(STDIN_PATH) {
        return STDIN_NAME.to_owned();
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leitura_respeita_o_teto() {
        let error = read_capped(&b"12345"[..], 4, "teste").expect_err("deve estourar o teto");
        assert!(
            matches!(error, InputError::TooLarge { limit: 4, .. }),
            "obtido: {error}"
        );
    }

    #[test]
    fn tamanho_exatamente_no_teto_e_aceito() {
        let bytes = read_capped(&b"1234"[..], 4, "teste").expect("cabe no teto");
        assert_eq!(bytes, b"1234");
    }
}

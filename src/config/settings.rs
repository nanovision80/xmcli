// Forma e validação da configuração.
//
// Este arquivo é a DEFINIÇÃO ÚNICA do formato de `config/defaults.json` e do arquivo de
// configuração do usuário. Ele é incluído por `build.rs` (via `include!`) para que um
// `defaults.json` inválido quebre a compilação, e não o programa em execução.
//
// Duas consequências de ser incluído em dois contextos:
//   - nada aqui pode depender de mais que `serde` e `thiserror`;
//   - os comentários de topo são `//`, não `//!`: doc de módulo interno não sobrevive a
//     `include!`. A documentação do módulo está na declaração `mod settings` em `mod.rs`.
use serde::Deserialize;
use thiserror::Error;

/// Menor taxa de amostragem aceita, em Hz (RF-306).
const SAMPLE_RATE_MIN_HZ: u32 = 22_050;

/// Maior taxa de amostragem aceita, em Hz (RF-306).
const SAMPLE_RATE_MAX_HZ: u32 = 192_000;

/// Menor latência aceita, em ms. Abaixo disso nenhum backend entrega sem xrun (RNF-02).
const LATENCY_MIN_MS: u16 = 1;

/// Maior latência aceita, em ms. Acima disso a compensação de RF-620 fica visível.
const LATENCY_MAX_MS: u16 = 500;

/// Menor histórico aceito no barramento master, em ms. Abaixo de uma janela de FFT o
/// espectro não teria o que analisar.
const MASTER_HISTORY_MIN_MS: u16 = 100;

/// Maior histórico aceito no barramento master, em ms. Mais que isso é memória parada: a
/// interface consome o passado recente, não o histórico da música.
const MASTER_HISTORY_MAX_MS: u16 = 5_000;

/// Menor fila de snapshots aceita. Abaixo disso um soluço da interface já perde estado.
const SNAPSHOT_CAPACITY_MIN: u16 = 8;

/// Maior fila de snapshots aceita. Além disso só se acumula estado velho, que ninguém mostra.
const SNAPSHOT_CAPACITY_MAX: u16 = 4_096;

/// Menor e maior taxa de quadros aceitas. O RF-601 pede de 30 a 60; a faixa aceita é mais
/// larga para quem quer economizar CPU ou tem um monitor rápido, mas sem chegar a zero, que
/// congelaria a imagem.
const FPS_MIN: u16 = 1;
const FPS_MAX: u16 = 240;

/// Menor orçamento de banda aceito, por quadro e por segundo, em bytes. Abaixo disso nem o
/// cromo de uma linha cabe.
const BYTES_MIN: u64 = 1_024;

/// Maior orçamento aceito por quadro, em bytes: um quadro truecolor de 400×120 células com cor
/// diferente em cada uma.
const FRAME_BYTES_MAX: u64 = 16 * 1_024 * 1_024;

/// Maior velocidade aceita para o marquee, em caracteres por segundo. Mais rápido que isso o
/// título passa sem que dê para ler.
const MARQUEE_SPEED_MAX: u16 = 60;

/// Maior pausa aceita nas pontas do marquee, em ms. Mais que isso o título parece parado.
const MARQUEE_PAUSE_MAX_MS: u16 = 10_000;

/// Maior passo de seek aceito, em segundos. Mais que isso a tecla pula a música inteira.
const SEEK_STEP_MAX_SECONDS: u16 = 600;

/// Maior percentual aceito; o menor é 1, porque zero anularia o que o percentual mede.
const PERCENT_MAX: u8 = 100;

/// Configuração resolvida do programa.
///
/// Depois de montada ela é **congelada**: nenhum laço quente lê configuração
/// (CLAUDE.md §2, invariante 7).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub audio: Audio,
    pub bus: Bus,
    pub io: Io,
    pub log: Log,
    pub ui: Ui,
}

/// Saída de áudio (RF-401, RF-402).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audio {
    /// Taxa de amostragem pedida ao dispositivo, em Hz.
    pub sample_rate: u32,
    /// Latência alvo do dispositivo, em milissegundos.
    pub latency_ms: u16,
}

/// Barramento de visualização (RF-309, RF-620).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bus {
    /// Quanto do passado recente o barramento master guarda, em milissegundos.
    ///
    /// Precisa cobrir o buffer do dispositivo — a interface consome o tempo *audível*, que
    /// fica atrás do mixado (RF-620) — mais a maior janela que uma visualização peça de uma
    /// vez. É o que decide quantos blocos cabem na fila.
    pub master_history_ms: u16,

    /// Quantos snapshots de estado cabem na fila antes de o produtor começar a descartar.
    ///
    /// A medida é em snapshots, e não em tempo, porque a produção segue os ticks da música:
    /// a taxa muda com o BPM e com o speed.
    pub snapshot_capacity: u16,
}

/// Leitura de arquivos de entrada (RF-105).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Io {
    /// Teto de tamanho para um arquivo de entrada, em bytes. Módulo é entrada hostil:
    /// sem teto, um cabeçalho mentiroso vira exaustão de memória.
    pub max_file_bytes: u64,
}

/// Registro de diagnóstico, sempre em `stderr` (RF-709).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Log {
    pub level: LogLevel,
}

/// Ritmo e banda da interface (RF-601, RF-621, RF-622, RNF-07).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    /// Nome do tema de cores, uma das entradas de `config/palettes.json` (RF-517).
    pub theme: String,
    /// Taxa de quadros que o pacing busca quando o terminal acompanha.
    pub fps_max: u16,
    /// Taxa abaixo da qual o quadro está caro demais e a cor desce um degrau.
    pub fps_min: u16,
    /// Teto de banda para o terminal, em bytes por segundo, mesmo que ele aguente mais.
    pub max_bytes_per_second: u64,
    /// Teto de bytes de um quadro só: o quadro maior que isso demora para chegar inteiro, e a
    /// imagem atrasa em relação ao som.
    pub max_frame_bytes: u64,
    /// Peso, em %, de cada medida nova na média do throughput do terminal. Alto reage rápido
    /// e oscila; baixo é estável e demora a notar que o link ficou lento.
    pub throughput_smoothing_percent: u8,
    /// Fração, em %, do throughput medido que a interface se permite usar. Usar tudo deixaria
    /// o terminal sempre no limite, e qualquer oscilação do link viraria atraso.
    pub throughput_headroom_percent: u8,
    /// Quadros seguidos dentro do orçamento antes de a cor subir um degrau.
    pub recover_frames: u32,
    /// Teto da espera por subir: cada subida que não se sustenta dobra a espera até aqui.
    pub recover_frames_max: u32,
    pub marquee: Marquee,
    /// Quanto cada toque nas setas avança ou recua a música, em segundos (RF-504).
    pub seek_step_seconds: u16,
    /// O que o display de tempo mostra ao abrir (RF-505).
    pub time_display: TimeDisplay,
}

/// O que o display de tempo do transporte mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeDisplay {
    /// Quanto já tocou.
    Elapsed,
    /// Quanto falta.
    Remaining,
}

impl TimeDisplay {
    /// O outro modo.
    pub fn toggled(self) -> Self {
        match self {
            Self::Elapsed => Self::Remaining,
            Self::Remaining => Self::Elapsed,
        }
    }
}

/// Rolagem do título no cabeçalho (RF-502).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marquee {
    /// Velocidade da rolagem, em caracteres por segundo.
    pub chars_per_second: u16,
    /// Quanto o título fica parado em cada ponta antes de seguir, em ms.
    pub pause_ms: u16,
}

/// Nível de detalhamento do log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Configuração sintaticamente válida, mas com valor fora do que o programa suporta.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{field} = {value} está fora da faixa suportada ({min} a {max})")]
pub struct OutOfRange {
    pub field: &'static str,
    pub value: u64,
    pub min: u64,
    pub max: u64,
}

impl Settings {
    /// Verifica os limites que o sistema de tipos não expressa.
    ///
    /// Roda em `build.rs` sobre os padrões e a cada resolução sobre o resultado final, de modo
    /// que um valor absurdo é recusado na borda e nunca chega ao dispositivo de áudio.
    pub fn validate(&self) -> Result<(), OutOfRange> {
        check_range(
            "audio.sample_rate",
            u64::from(self.audio.sample_rate),
            u64::from(SAMPLE_RATE_MIN_HZ),
            u64::from(SAMPLE_RATE_MAX_HZ),
        )?;
        check_range(
            "audio.latency_ms",
            u64::from(self.audio.latency_ms),
            u64::from(LATENCY_MIN_MS),
            u64::from(LATENCY_MAX_MS),
        )?;
        check_range(
            "bus.master_history_ms",
            u64::from(self.bus.master_history_ms),
            u64::from(MASTER_HISTORY_MIN_MS),
            u64::from(MASTER_HISTORY_MAX_MS),
        )?;
        check_range(
            "bus.snapshot_capacity",
            u64::from(self.bus.snapshot_capacity),
            u64::from(SNAPSHOT_CAPACITY_MIN),
            u64::from(SNAPSHOT_CAPACITY_MAX),
        )?;
        check_range("io.max_file_bytes", self.io.max_file_bytes, 1, u64::MAX)?;
        self.ui.validate()
    }
}

impl Ui {
    fn validate(&self) -> Result<(), OutOfRange> {
        check_range(
            "ui.fps_max",
            u64::from(self.fps_max),
            u64::from(FPS_MIN),
            u64::from(FPS_MAX),
        )?;
        check_range(
            "ui.fps_min",
            u64::from(self.fps_min),
            u64::from(FPS_MIN),
            u64::from(self.fps_max),
        )?;
        check_range(
            "ui.max_bytes_per_second",
            self.max_bytes_per_second,
            BYTES_MIN,
            u64::MAX,
        )?;
        check_range(
            "ui.max_frame_bytes",
            self.max_frame_bytes,
            BYTES_MIN,
            FRAME_BYTES_MAX,
        )?;
        check_range(
            "ui.throughput_smoothing_percent",
            u64::from(self.throughput_smoothing_percent),
            1,
            u64::from(PERCENT_MAX),
        )?;
        check_range(
            "ui.throughput_headroom_percent",
            u64::from(self.throughput_headroom_percent),
            1,
            u64::from(PERCENT_MAX),
        )?;
        check_range(
            "ui.recover_frames",
            u64::from(self.recover_frames),
            1,
            u64::from(self.recover_frames_max),
        )?;
        check_range(
            "ui.recover_frames_max",
            u64::from(self.recover_frames_max),
            1,
            u64::from(u32::MAX),
        )?;
        check_range(
            "ui.marquee.chars_per_second",
            u64::from(self.marquee.chars_per_second),
            1,
            u64::from(MARQUEE_SPEED_MAX),
        )?;
        check_range(
            "ui.marquee.pause_ms",
            u64::from(self.marquee.pause_ms),
            0,
            u64::from(MARQUEE_PAUSE_MAX_MS),
        )?;
        check_range(
            "ui.seek_step_seconds",
            u64::from(self.seek_step_seconds),
            1,
            u64::from(SEEK_STEP_MAX_SECONDS),
        )
    }
}

fn check_range(field: &'static str, value: u64, min: u64, max: u64) -> Result<(), OutOfRange> {
    if (min..=max).contains(&value) {
        return Ok(());
    }
    Err(OutOfRange {
        field,
        value,
        min,
        max,
    })
}

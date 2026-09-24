//! Ponto de entrada: monta as peças e sai do caminho.

#![forbid(unsafe_code)]

mod cli;

use std::io::{ErrorKind, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{CommandFactory, Parser};

use xmcli::audio::{device, offline};
use xmcli::config::{self, LogLevel, Settings};
use xmcli::formats::{self, FormatError};
use xmcli::info::Report;
use xmcli::io::{self, Input};
use xmcli::player;
use xmcli::ui::chrome::marquee;
use xmcli::ui::term::color::{self, ColorDepth};
use xmcli::ui::transport::Transport;
use xmcli::ui::{self, Exit, Screen, Setup};

use crate::cli::{Args, ExitCode};

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("xmcli: {error:#}");
            ExitCode::Runtime.into()
        }
    }
}

fn run(args: &Args) -> anyhow::Result<ExitCode> {
    let user = config::read_user_file(args.config.as_deref())?;
    let settings = config::resolve(user.as_ref(), std::env::vars(), &args.patch())?;
    init_tracing(&settings);
    tracing::debug!(?settings, "configuração resolvida");
    let colors = color::detect(|name| std::env::var_os(name), args.mono);
    tracing::debug!(?colors, "profundidade de cor detectada");

    if args.inputs.is_empty() {
        eprint!("{}", Args::command().render_help());
        return Ok(ExitCode::Usage);
    }
    if args.info {
        return report_all(args, &settings);
    }
    play_all(args, &settings, colors)
}

// ---------------------------------------------------------------- metadados

fn report_all(args: &Args, settings: &Settings) -> anyhow::Result<ExitCode> {
    let mut out = std::io::stdout().lock();
    let mut worst = ExitCode::Success;
    let mut collected = Vec::new();

    for path in io::expand(&args.inputs)? {
        let input = match load(&path, settings) {
            Ok(input) => input,
            Err(error) => {
                worst = report_failure(&path, &error, worst);
                continue;
            }
        };
        let song = match formats::read(&input.bytes) {
            Ok(song) => song,
            Err(error) => {
                worst = report_failure(&path, &error.into(), worst);
                continue;
            }
        };
        let report = Report {
            file: input.name,
            song,
        };

        if args.json {
            collected.push(report);
        } else if !write_line(&mut out, &report.to_text())? {
            return Ok(worst);
        }
    }

    // Em JSON a saída é sempre um array, mesmo para um arquivo só: forma previsível vale mais
    // para quem consome com script do que a economia de dois colchetes.
    if args.json {
        write_line(&mut out, &serde_json::to_string_pretty(&collected)?)?;
    }
    Ok(worst)
}

// ---------------------------------------------------------------- reprodução

fn play_all(args: &Args, settings: &Settings, colors: ColorDepth) -> anyhow::Result<ExitCode> {
    let inputs = io::expand(&args.inputs)?;
    if args.render.is_some() && inputs.len() > 1 {
        anyhow::bail!(
            "--render aceita um módulo por vez, e foram informados {}",
            inputs.len()
        );
    }

    let theme = config::theme(settings);
    let keymap = config::keymap();
    let setup = Setup {
        settings,
        theme: &theme,
        keymap: &keymap,
        colors,
    };
    // A interface só sobe num terminal: com a saída num arquivo ou num cano, o player toca
    // sem ela, como antes.
    let interactive = args.render.is_none() && !args.raw_stdout && std::io::stdout().is_terminal();
    let mut screen = interactive.then(|| Screen::open(&setup)).transpose()?;

    // Com a tela aberta, texto em `stderr` cairia no meio do quadro e sumiria no seguinte: o
    // que há para dizer espera a tela fechar.
    let mut worst = ExitCode::Success;
    let mut deferred = Vec::new();
    let mut underrun_frames = 0;
    let mut index = 0;
    let mut state = Transport::Playing;
    while let Some(path) = inputs.get(index) {
        let ui = screen.as_mut().map(|screen| (screen, &setup));
        let exit = match play_one(path, args, settings, ui, state) {
            Ok(track) => {
                underrun_frames += track.underrun_frames;
                track.exit
            }
            Err(error) => {
                if screen.is_some() {
                    deferred.push((path.clone(), error));
                } else {
                    worst = report_failure(path, &error, worst);
                }
                Exit::Next
            }
        };
        (index, state) = match exit {
            // Sair é sair da lista inteira, não pular para a próxima faixa.
            Exit::Quit => break,
            Exit::Ended | Exit::Next => (index + 1, Transport::Playing),
            // Na primeira faixa, a anterior é ela mesma.
            Exit::Previous => (index.saturating_sub(1), Transport::Playing),
            Exit::Rewind(state) => (index, state),
        };
    }
    drop(screen);

    for (path, error) in &deferred {
        worst = report_failure(path, error, worst);
    }
    if underrun_frames > 0 {
        // Estouro é sintoma, não detalhe: o usuário precisa saber para elevar a latência.
        tracing::warn!(
            frames = underrun_frames,
            "o dispositivo ficou sem dados; eleve audio.latency_ms"
        );
    }
    Ok(worst)
}

/// Como terminou uma faixa.
struct Track {
    exit: Exit,
    /// Quadros em que o dispositivo ficou sem dados (RF-402).
    underrun_frames: u64,
}

fn play_one(
    path: &Path,
    args: &Args,
    settings: &Settings,
    ui: Option<(&mut Screen, &Setup)>,
    state: Transport,
) -> anyhow::Result<Track> {
    let input = load(path, settings)?;
    let song = formats::read(&input.bytes)?;
    let rate = settings.audio.sample_rate;
    let title = if song.title.is_empty() {
        input.name.clone()
    } else {
        song.title.clone()
    };
    // Com a interface aberta quem mostra a faixa é o marquee, e a linha iria para o meio dela.
    let quiet = ui.is_some();
    let announce = |seconds: f64| {
        if quiet {
            return;
        }
        eprintln!(
            "xmcli: {title} [{} {}ch] {}",
            song.dialect.extension(),
            song.channels,
            format_duration(seconds),
        );
    };
    let ended = Track {
        exit: Exit::Ended,
        underrun_frames: 0,
    };

    if args.render.is_some() || args.raw_stdout {
        let mut engine = player::load(&input.bytes, rate)?;
        announce(engine.duration_seconds());
        if let Some(target) = &args.render {
            render_to_file(engine.as_mut(), rate, target)?;
            return Ok(ended);
        }
        let mut out = std::io::stdout().lock();
        offline::render_raw(engine.as_mut(), rate, &mut out)?;
        return Ok(ended);
    }

    // O motor nasce na linha de áudio, que é quem o usa: ele não atravessa linhas (Engine).
    let bytes = input.bytes;
    let mut playback = device::start(
        move || player::load(&bytes, rate),
        rate,
        settings.audio.latency_ms,
        args.device.clone(),
        state != Transport::Playing,
    )?;
    announce(playback.duration_seconds);
    let exit = match ui {
        Some((screen, setup)) => {
            let marquee = marquee::text(&song.title, &file_name(&input.name));
            ui::run(screen, &mut playback, setup, &marquee, state)
        }
        None => Ok(Exit::Ended),
    };
    if exit.is_err() {
        // A interface caiu: sem ela ninguém mais pediria para parar, e o erro só apareceria
        // depois de a música tocar até o fim.
        playback.stop();
    }
    let played = playback.wait()?;
    Ok(Track {
        exit: exit?,
        underrun_frames: played.underrun_frames,
    })
}

fn render_to_file(
    engine: &mut dyn player::Engine,
    rate: u32,
    target: &PathBuf,
) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(target)?;
    let rendered = offline::render_wav(engine, rate, &mut file)?;
    eprintln!(
        "xmcli: {} escrito ({} a {} Hz)",
        target.display(),
        format_duration(rendered.seconds()),
        rendered.sample_rate,
    );
    Ok(())
}

// ---------------------------------------------------------------- utilidades

fn load(path: &Path, settings: &Settings) -> anyhow::Result<Input> {
    Ok(io::read(path, settings.io.max_file_bytes)?)
}

/// O nome do arquivo sem o diretório; para a entrada padrão, o nome que ela já tem.
fn file_name(name: &str) -> String {
    Path::new(name).file_name().map_or_else(
        || name.to_owned(),
        |file| file.to_string_lossy().into_owned(),
    )
}

/// Relata a falha de um arquivo sem derrubar a lista, e agrava o código de saída (RF-707).
///
/// O usuário apontou para um diretório inteiro: interromper tudo no primeiro arquivo ruim
/// esconderia os bons.
fn report_failure(path: &Path, error: &anyhow::Error, current: ExitCode) -> ExitCode {
    eprintln!("xmcli: {}: {error:#}", path.display());
    let unsupported = matches!(error.downcast_ref(), Some(FormatError::Unsupported));
    match (current, unsupported) {
        (ExitCode::Runtime, _) | (_, false) => ExitCode::Runtime,
        _ => ExitCode::Unsupported,
    }
}

/// Escreve uma linha em `stdout`, respondendo `false` quando o consumidor fechou o cano.
///
/// `xmcli --info | head` é uso corriqueiro, e o `println!` da biblioteca padrão entra em
/// pânico nesse caso. Encerrar em silêncio é o comportamento que se espera de um utilitário.
fn write_line(out: &mut impl Write, text: &str) -> anyhow::Result<bool> {
    match writeln!(out, "{text}") {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Formata uma duração como `m:ss`.
fn format_duration(seconds: f64) -> String {
    const SECONDS_PER_MINUTE: u64 = 60;
    let total = seconds.max(0.0) as u64;
    format!(
        "{}:{:02}",
        total / SECONDS_PER_MINUTE,
        total % SECONDS_PER_MINUTE
    )
}

/// Liga o log em `stderr`, nunca em `stdout` — a TUI vai morar lá (RF-709).
fn init_tracing(settings: &Settings) {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from(LevelBridge(settings.log.level)))
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

/// Ponte entre o nível vindo da configuração e o do `tracing`.
///
/// Os dois tipos são de crates diferentes, então a conversão precisa de um tipo local.
struct LevelBridge(LogLevel);

impl From<LevelBridge> for tracing::Level {
    fn from(bridge: LevelBridge) -> Self {
        match bridge.0 {
            LogLevel::Error => Self::ERROR,
            LogLevel::Warn => Self::WARN,
            LogLevel::Info => Self::INFO,
            LogLevel::Debug => Self::DEBUG,
            LogLevel::Trace => Self::TRACE,
        }
    }
}

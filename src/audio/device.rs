//! Saída em tempo real pelo dispositivo do sistema (RF-401, RF-402).
//!
//! O motor **não** roda dentro do callback do dispositivo. Quem renderiza é uma linha
//! alimentadora que escreve em um anel SPSC; o callback só copia do anel para o dispositivo.
//! Isso mantém o invariante 1 do CLAUDE.md §2 mesmo com um motor que aloca por conta própria,
//! e é o que permitirá trocar o motor na etapa 8 sem tocar no caminho de tempo real.

use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use thiserror::Error;

use crate::audio::clock::Clock;
use crate::player::{CHANNELS, Engine, EngineError};

/// Quantas vezes a latência alvo cabe no anel.
///
/// Três dá folga para a linha alimentadora perder o passo sem o dispositivo ficar sem dados,
/// sem inflar a latência: o dispositivo consome do início do anel, não do fim.
const RING_LATENCY_MULTIPLE: u32 = 3;

/// Quadros por chamada ao motor na linha alimentadora.
const BLOCK_FRAMES: usize = 1_024;

/// Quanto a linha alimentadora dorme quando o anel está cheio.
///
/// Curto o bastante para nunca ser o gargalo, longo o bastante para não virar espera ativa.
const FEEDER_IDLE: Duration = Duration::from_millis(2);

/// Falha na saída de áudio.
#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("nenhum dispositivo de saída disponível")]
    NoDevice,

    #[error("dispositivo \"{0}\" não encontrado")]
    NotFound(String),

    #[error("não foi possível consultar o dispositivo de saída")]
    Query(#[source] cpal::Error),

    #[error("o dispositivo não aceitou a configuração pedida")]
    Build(#[source] cpal::Error),

    #[error("não foi possível iniciar a reprodução")]
    Start(#[source] cpal::Error),

    #[error(transparent)]
    Engine(EngineError),

    #[error("não foi possível criar a linha de áudio")]
    Thread(#[source] std::io::Error),

    #[error("a linha de áudio terminou em pânico")]
    Panicked,
}

/// Como a reprodução terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub frames: u64,
    pub underrun_frames: u64,
}

/// Pedido da interface à linha de áudio (etapa 2.6).
///
/// Os comandos entram com quem os envia: parar e pausar com o transporte; seek e volume com
/// a barra de seek e os sliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Parar agora, sem esperar o anel esvaziar.
    Stop,
    /// Tocar silêncio no lugar da música, sem perder o que está no anel.
    Pause,
    /// Voltar a tocar do ponto exato em que pausou.
    Resume,
}

/// Comandos que cabem na fila antes de o remetente desistir de enfileirar.
///
/// A fila é esvaziada a cada bloco do motor — uns 23 ms a 44,1 kHz —, e ninguém aperta tecla
/// tão rápido. Cheia, ela só pode estar cheia de pedidos que ainda vão ser atendidos.
const COMMAND_CAPACITY: usize = 16;

/// Nome da linha alimentadora, para quem a vir num depurador.
const FEEDER_THREAD_NAME: &str = "xmcli-audio";

/// Milissegundos em um segundo, para converter a latência em quadros.
const MS_PER_SECOND: usize = 1_000;

/// Uma reprodução em andamento, numa linha de execução própria.
pub struct Playback {
    /// O relógio do dispositivo: o que já soou e o que está soando (RF-406, RF-620).
    pub clock: Arc<Clock>,
    /// Duração estimada da música, em segundos.
    pub duration_seconds: f64,
    commands: HeapProd<Command>,
    thread: JoinHandle<Result<Played, DeviceError>>,
}

impl Playback {
    /// Pede para parar. A reprodução termina no próximo bloco do motor.
    pub fn stop(&mut self) {
        // Fila cheia é fila com pedidos por atender; um Stop a mais não mudaria nada.
        let _ = self.commands.try_push(Command::Stop);
    }

    /// Envia um pedido à linha de áudio; `false` se a fila estava cheia e ele não entrou.
    ///
    /// Quem envia pausa ou retomada precisa saber: o estado que ele mostra só muda se o
    /// pedido entrou.
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.try_push(command).is_ok()
    }

    /// Se a reprodução já acabou — a música chegou ao fim ou alguém a parou.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Espera a reprodução terminar.
    pub fn wait(self) -> Result<Played, DeviceError> {
        self.thread.join().map_err(|_| DeviceError::Panicked)?
    }
}

/// Começa a tocar numa linha de execução própria e volta assim que o som começou.
///
/// Com `paused`, o dispositivo abre tocando silêncio e a música espera um [`Command::Resume`]
/// no primeiro quadro — é assim que o player fica parado no começo da faixa.
///
/// O motor é construído lá dentro, por `load`: ele não é `Send` (ver [`Engine`]), e o fluxo do
/// dispositivo também não é em todas as plataformas. Falha ao carregar ou ao abrir o
/// dispositivo volta daqui, antes de haver reprodução.
pub fn start(
    load: impl FnOnce() -> Result<Box<dyn Engine>, EngineError> + Send + 'static,
    sample_rate: u32,
    latency_ms: u16,
    wanted: Option<String>,
    paused: bool,
) -> Result<Playback, DeviceError> {
    let clock = Clock::new(sample_rate);
    clock.set_paused(paused);
    let (commands, pending) = HeapRb::<Command>::new(COMMAND_CAPACITY).split();
    let (ready, started) = mpsc::sync_channel(1);

    let feeder = Feeder {
        clock: Arc::clone(&clock),
        commands: pending,
        sample_rate,
        latency_ms,
    };
    let thread = std::thread::Builder::new()
        .name(FEEDER_THREAD_NAME.to_owned())
        .spawn(move || feeder.run(load, wanted.as_deref(), &ready))
        .map_err(DeviceError::Thread)?;

    match started.recv() {
        Ok(Ok(duration_seconds)) => Ok(Playback {
            clock,
            duration_seconds,
            commands,
            thread,
        }),
        Ok(Err(error)) => Err(error),
        // A linha morreu antes de avisar: só um pânico faz isso.
        Err(_) => Err(DeviceError::Panicked),
    }
}

/// A linha que renderiza o motor e alimenta o anel do dispositivo.
struct Feeder {
    clock: Arc<Clock>,
    commands: HeapCons<Command>,
    sample_rate: u32,
    latency_ms: u16,
}

impl Feeder {
    fn run(
        mut self,
        load: impl FnOnce() -> Result<Box<dyn Engine>, EngineError>,
        wanted: Option<&str>,
        ready: &SyncSender<Result<f64, DeviceError>>,
    ) -> Result<Played, DeviceError> {
        let opened = load().map_err(DeviceError::Engine).and_then(|mut engine| {
            let (stream, producer) = self.open(wanted)?;
            Ok((engine.duration_seconds(), engine, stream, producer))
        });
        let (duration, mut engine, stream, mut producer) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                // Quem espera pelo aviso é `start`, e ele devolve o erro a quem chamou.
                let _ = ready.send(Err(error));
                return Ok(Played {
                    frames: 0,
                    underrun_frames: 0,
                });
            }
        };
        let _ = ready.send(Ok(duration));

        let mut block = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
        let mut rendered_frames = 0_u64;
        let mut stopped = false;
        while !stopped {
            let frames = engine.render(&mut block);
            if frames == 0 {
                break;
            }
            rendered_frames += frames as u64;
            stopped = !self.push_all(&mut producer, &block[..frames * CHANNELS]);
        }
        self.clock.mark_finished();

        // Espera o dispositivo consumir o que ainda está no anel antes de encerrar o fluxo —
        // a não ser que tenham pedido para parar, e aí o resto do anel não interessa.
        while !stopped && self.clock.frames_played() < rendered_frames {
            stopped = self.stop_requested();
            std::thread::sleep(FEEDER_IDLE);
        }
        drop(stream);

        Ok(Played {
            frames: rendered_frames,
            underrun_frames: self.clock.underruns(),
        })
    }

    /// Abre o fluxo do dispositivo, já tocando, e devolve o lado que o alimenta.
    fn open(&self, wanted: Option<&str>) -> Result<(cpal::Stream, HeapProd<f32>), DeviceError> {
        let device = pick(wanted)?;
        let config = cpal::StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: self.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let latency_frames =
            self.sample_rate as usize * usize::from(self.latency_ms) / MS_PER_SECOND;
        let capacity = latency_frames * RING_LATENCY_MULTIPLE as usize * CHANNELS;
        let (producer, mut consumer) =
            HeapRb::<f32>::new(capacity.max(BLOCK_FRAMES * CHANNELS)).split();

        let callback_clock = Arc::clone(&self.clock);
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| feed_device(out, &mut consumer, &callback_clock),
                |error| tracing::warn!(%error, "erro no fluxo de áudio"),
                None,
            )
            .map_err(DeviceError::Build)?;

        stream.play().map_err(DeviceError::Start)?;
        Ok((stream, producer))
    }

    /// Empurra o bloco inteiro para o anel, esperando enquanto ele estiver cheio.
    ///
    /// Devolve `false` se pediram para parar no meio da espera.
    fn push_all(&mut self, producer: &mut impl Producer<Item = f32>, mut samples: &[f32]) -> bool {
        while !samples.is_empty() {
            if self.stop_requested() {
                return false;
            }
            let pushed = producer.push_slice(samples);
            samples = &samples[pushed..];
            if pushed == 0 {
                std::thread::sleep(FEEDER_IDLE);
            }
        }
        true
    }

    /// Atende a fila de comandos e diz se algum deles pediu para parar.
    fn stop_requested(&mut self) -> bool {
        let mut stop = false;
        while let Some(command) = self.commands.try_pop() {
            match command {
                Command::Stop => stop = true,
                Command::Pause => self.clock.set_paused(true),
                Command::Resume => self.clock.set_paused(false),
            }
        }
        stop
    }
}

/// O callback do dispositivo.
///
/// Único trabalho dele: copiar do anel para a saída. Sem alocação, sem lock, sem I/O
/// (CLAUDE.md §2, invariante 1); os sinalizadores do relógio que ele lê são átomos.
fn feed_device(out: &mut [f32], ring: &mut impl Consumer<Item = f32>, clock: &Clock) {
    if clock.is_paused() {
        // O anel fica como está e nada é entregue: retomar continua da amostra seguinte, e o
        // relógio não anda enquanto nada soa. Este silêncio foi pedido; não é estouro.
        out.fill(0.0);
        return;
    }
    let taken = ring.pop_slice(out);
    if taken < out.len() {
        // Silêncio em vez de lixo: um estouro deve soar como um buraco, não como um estalo. A
        // contagem sobe para o programa poder relatar (RF-402).
        out[taken..].fill(0.0);

        // Depois do fim da música o anel esvazia por definição, e o último bloco é parcial
        // porque o total renderizado não é múltiplo do buffer do dispositivo. Contar isso como
        // estouro faria o programa pedir mais latência para um problema que não existe.
        if !clock.is_finished() {
            clock.record_underrun(((out.len() - taken) / CHANNELS) as u64);
        }
    }
    clock.deliver((taken / CHANNELS) as u64);
}

/// Escolhe o dispositivo pedido, ou o padrão do sistema (RF-401).
fn pick(wanted: Option<&str>) -> Result<cpal::Device, DeviceError> {
    let host = cpal::default_host();
    let Some(wanted) = wanted else {
        return host.default_output_device().ok_or(DeviceError::NoDevice);
    };

    let mut devices = host.output_devices().map_err(DeviceError::Query)?;
    devices
        .find(|device| {
            device
                .description()
                .is_ok_and(|description| description.name().contains(wanted))
        })
        .ok_or_else(|| DeviceError::NotFound(wanted.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::Observer;

    const RATE: u32 = 44_100;

    /// Quadros por chamada do callback simulado.
    const CALLBACK_FRAMES: usize = 4;

    /// Um anel com `frames` quadros de amostras não nulas, que dá para distinguir de silêncio.
    fn ring(frames: usize) -> HeapCons<f32> {
        let (mut producer, consumer) = HeapRb::<f32>::new((frames * CHANNELS).max(1)).split();
        producer.push_iter((1..=frames * CHANNELS).map(|sample| sample as f32));
        consumer
    }

    #[test]
    fn tocando_copia_do_anel_e_entrega() {
        let clock = Clock::new(RATE);
        let mut consumer = ring(CALLBACK_FRAMES);
        let mut out = [0.0; CALLBACK_FRAMES * CHANNELS];

        feed_device(&mut out, &mut consumer, &clock);
        assert_eq!(out[0], 1.0);
        assert_eq!(clock.frames_played(), CALLBACK_FRAMES as u64);
        assert_eq!(clock.underruns(), 0);
    }

    #[test]
    fn pausado_toca_silencio_sem_consumir_nem_entregar() {
        let clock = Clock::new(RATE);
        let mut consumer = ring(CALLBACK_FRAMES);
        let mut out = [1.0; CALLBACK_FRAMES * CHANNELS];

        clock.set_paused(true);
        feed_device(&mut out, &mut consumer, &clock);
        assert!(out.iter().all(|&sample| sample == 0.0));
        assert_eq!(consumer.occupied_len(), CALLBACK_FRAMES * CHANNELS);
        assert_eq!(clock.frames_played(), 0);
        // Pausa não é o dispositivo ficando sem dados.
        assert_eq!(clock.underruns(), 0);

        // Retomado, continua da primeira amostra que estava esperando.
        clock.set_paused(false);
        feed_device(&mut out, &mut consumer, &clock);
        assert_eq!(out[0], 1.0);
        assert_eq!(clock.frames_played(), CALLBACK_FRAMES as u64);
    }

    #[test]
    fn estouro_parcial_entrega_so_a_musica() {
        let clock = Clock::new(RATE);
        let mut consumer = ring(CALLBACK_FRAMES / 2);
        let mut out = [1.0; CALLBACK_FRAMES * CHANNELS];

        feed_device(&mut out, &mut consumer, &clock);
        assert_eq!(clock.frames_played(), (CALLBACK_FRAMES / 2) as u64);
        assert_eq!(clock.underruns(), (CALLBACK_FRAMES / 2) as u64);
        assert!(out[CALLBACK_FRAMES..].iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn anel_vazio_e_estouro_so_antes_do_fim() {
        let clock = Clock::new(RATE);
        let mut consumer = ring(0);
        let mut out = [1.0; CALLBACK_FRAMES * CHANNELS];

        feed_device(&mut out, &mut consumer, &clock);
        assert!(out.iter().all(|&sample| sample == 0.0));
        assert_eq!(clock.underruns(), CALLBACK_FRAMES as u64);
        // O silêncio do estouro não é música entregue.
        assert_eq!(clock.frames_played(), 0);

        clock.mark_finished();
        feed_device(&mut out, &mut consumer, &clock);
        assert_eq!(clock.underruns(), CALLBACK_FRAMES as u64);
    }
}

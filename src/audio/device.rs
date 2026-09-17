//! Saída em tempo real pelo dispositivo do sistema (RF-401, RF-402).
//!
//! O motor **não** roda dentro do callback do dispositivo. Quem renderiza é uma linha
//! alimentadora que escreve em um anel SPSC; o callback só copia do anel para o dispositivo.
//! Isso mantém o invariante 1 do CLAUDE.md §2 mesmo com um motor que aloca por conta própria,
//! e é o que permitirá trocar o motor na etapa 8 sem tocar no caminho de tempo real.

use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};
use thiserror::Error;

use crate::audio::clock::Clock;
use crate::player::{CHANNELS, Engine};

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
}

/// Como a reprodução terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    pub frames: u64,
    pub underrun_frames: u64,
}

/// Toca a música inteira no dispositivo e só volta quando o último quadro saiu.
pub fn play(
    mut engine: Box<dyn Engine>,
    sample_rate: u32,
    latency_ms: u16,
    wanted: Option<&str>,
) -> Result<Played, DeviceError> {
    let device = pick(wanted)?;
    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let latency_frames = sample_rate as usize * usize::from(latency_ms) / 1_000;
    let capacity = latency_frames * RING_LATENCY_MULTIPLE as usize * CHANNELS;
    let (mut producer, mut consumer) =
        HeapRb::<f32>::new(capacity.max(BLOCK_FRAMES * CHANNELS)).split();

    let clock = Clock::new(sample_rate);
    let callback_clock = Arc::clone(&clock);

    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], _| {
                // Único trabalho do callback: copiar. Sem alocação, sem lock, sem I/O.
                let taken = consumer.pop_slice(out);
                if taken < out.len() {
                    // Silêncio em vez de lixo: um estouro deve soar como um buraco, não como
                    // um estalo. A contagem sobe para o programa poder relatar (RF-402).
                    out[taken..].fill(0.0);

                    // Depois do fim da música o anel esvazia por definição, e o último bloco
                    // é parcial porque o total renderizado não é múltiplo do buffer do
                    // dispositivo. Contar isso como estouro faria o programa pedir mais
                    // latência para um problema que não existe. Ler o sinalizador aqui é
                    // barato e não fere o invariante 1: é um átomo, sem lock e sem alocação.
                    if !callback_clock.is_finished() {
                        callback_clock.record_underrun(((out.len() - taken) / CHANNELS) as u64);
                    }
                }
                callback_clock.deliver((out.len() / CHANNELS) as u64);
            },
            |error| tracing::warn!(%error, "erro no fluxo de áudio"),
            None,
        )
        .map_err(DeviceError::Build)?;

    stream.play().map_err(DeviceError::Start)?;

    let mut block = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
    let mut rendered_frames = 0_u64;
    loop {
        let frames = engine.render(&mut block);
        if frames == 0 {
            clock.mark_finished();
            break;
        }
        rendered_frames += frames as u64;
        push_all(&mut producer, &block[..frames * CHANNELS]);
    }

    // Espera o dispositivo consumir o que ainda está no anel antes de encerrar o fluxo.
    while clock.frames_played() < rendered_frames {
        std::thread::sleep(FEEDER_IDLE);
    }
    drop(stream);

    Ok(Played {
        frames: rendered_frames,
        underrun_frames: clock.underruns(),
    })
}

/// Empurra o bloco inteiro para o anel, cedendo a vez enquanto ele estiver cheio.
fn push_all(producer: &mut impl Producer<Item = f32>, mut samples: &[f32]) {
    while !samples.is_empty() {
        let pushed = producer.push_slice(samples);
        samples = &samples[pushed..];
        if pushed == 0 {
            std::thread::sleep(FEEDER_IDLE);
        }
    }
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

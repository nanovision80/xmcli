//! O snapshot de estado sobre um módulo real (RF-521).
//!
//! Um módulo sintético prova que o campo existe; só um módulo de verdade prova que ele se
//! move. Este exercita o caminho inteiro da etapa 3.3: o motor responde o estado, o laço de
//! render carimba, o barramento entrega e o consumidor lê.

#![cfg(feature = "engine-openmpt")]

use xmcli::bus::Stamped;
use xmcli::bus::snapshot::{self, Snapshot};
use xmcli::player::{self, CHANNELS};

const SAMPLE_RATE: u32 = 44_100;
/// Quadros por chamada ao motor, como no laço de render.
const BLOCK_FRAMES: usize = 1_024;
/// Quanto da música tocar. Dois segundos passam por várias linhas em qualquer andamento.
const SECONDS: usize = 2;
/// Canais declarados por `samples/486.xm`.
const EXPECTED_CHANNELS: u16 = 4;
/// Faixa de andamento que os quatro formatos aceitam.
const BPM_RANGE: std::ops::RangeInclusive<u16> = 32..=255;

/// Um XM de licença pública, de 1992, com quatro canais.
const MODULE: &[u8] = include_bytes!("../samples/486.xm");

/// Toca o começo do módulo publicando um snapshot por bloco renderizado.
fn collect() -> Vec<Stamped<Snapshot>> {
    let mut engine = player::load(MODULE, SAMPLE_RATE).expect("o motor deve carregar o módulo");
    let blocks = SAMPLE_RATE as usize * SECONDS / BLOCK_FRAMES;
    let (mut sender, mut receiver) = snapshot::channel(u16::try_from(blocks + 1).expect("cabe"));

    let mut buffer = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
    let mut at_frame = 0_u64;
    for _ in 0..blocks {
        let frames = engine.render(&mut buffer);
        assert_ne!(frames, 0, "a música acabou antes de {SECONDS}s");
        at_frame += frames as u64;
        sender.send(at_frame, engine.snapshot());
    }

    let mut collected = Vec::new();
    while let Some(stamped) = receiver.recv() {
        collected.push(stamped);
    }
    assert_eq!(
        receiver.dropped(),
        0,
        "a fila comportava tudo que foi enviado"
    );
    collected
}

#[test]
fn o_estado_acompanha_a_musica() {
    let snapshots = collect();

    let first = snapshots.first().expect("pelo menos um snapshot").payload;
    assert_eq!(first.channels, EXPECTED_CHANNELS);
    assert!(first.speed >= 1, "speed é ticks por linha, nunca zero");
    assert!(
        BPM_RANGE.contains(&first.bpm),
        "andamento fora da faixa dos formatos: {}",
        first.bpm
    );

    let rows: Vec<u16> = snapshots
        .iter()
        .map(|stamped| stamped.payload.row)
        .collect();
    assert!(
        rows.iter().any(|row| *row != rows[0]),
        "a linha não andou em {SECONDS}s de música"
    );
}

#[test]
fn os_carimbos_crescem_com_o_render() {
    let snapshots = collect();
    let mut previous = 0_u64;
    for stamped in &snapshots {
        assert!(
            stamped.at_frame > previous,
            "carimbo não avançou: {} depois de {previous}",
            stamped.at_frame
        );
        previous = stamped.at_frame;
    }
}

#[test]
fn os_canais_soam() {
    let snapshots = collect();
    let loudest = snapshots
        .iter()
        .flat_map(|stamped| stamped.payload.levels().to_vec())
        .map(|vu| vu.left.max(vu.right))
        .fold(0.0_f32, f32::max);

    // O limiar separa som de ruído numérico: um caminho quebrado entrega zero absoluto.
    assert!(loudest > 0.01, "nenhum canal soou (pico {loudest})");
}

#[test]
fn os_niveis_so_cobrem_os_canais_do_modulo() {
    let snapshots = collect();
    for stamped in &snapshots {
        assert_eq!(
            stamped.payload.levels().len(),
            usize::from(EXPECTED_CHANNELS)
        );
    }
}

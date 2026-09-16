//! Caminho completo de reprodução: módulo em bytes, motor, renderização e WAV.
//!
//! Diferente de `tests/formats.rs`, aqui o módulo carrega amostra e padrão de verdade — sem
//! isso o resultado seria silêncio válido, e um silêncio não prova que a cadeia funciona.

#![cfg(feature = "engine-openmpt")]

use xmcli::audio::offline;
use xmcli::player;

const SAMPLE_RATE: u32 = 44_100;
/// 32 bytes de amostra, declarados em *words* como o formato exige.
const SAMPLE_WORDS: u16 = 16;
/// Períodos Amiga de C-2, E-2, G-2 e C-3.
const NOTES: [(usize, u16); 4] = [(0, 428), (16, 339), (32, 285), (48, 214)];
/// Efeito `Cxx` do ProTracker: define o volume do canal.
const EFFECT_SET_VOLUME: u8 = 0x0C;

fn put(buffer: &mut Vec<u8>, offset: usize, bytes: &[u8]) {
    let end = offset + bytes.len();
    if buffer.len() < end {
        buffer.resize(end, 0);
    }
    buffer[offset..end].copy_from_slice(bytes);
}

/// Um MOD de quatro notas sobre uma senoide em loop.
fn audible_module() -> Vec<u8> {
    let mut module = vec![0_u8; 1084];
    put(&mut module, 0, b"audible test");
    put(&mut module, 20, b"sine loop");
    put(&mut module, 42, &SAMPLE_WORDS.to_be_bytes());
    module[45] = 64; // volume da amostra
    put(&mut module, 48, &SAMPLE_WORDS.to_be_bytes()); // loop do início ao fim
    module[950] = 1; // uma posição na sequência
    put(&mut module, 1080, b"M.K.");

    // Padrão 0: 64 linhas × 4 canais × 4 bytes por célula.
    let mut pattern = vec![0_u8; 64 * 4 * 4];
    for (row, period) in NOTES {
        let cell = row * 16; // primeira célula da linha, canal 0
        let sample = 1_u8;
        pattern[cell] = (sample & 0xF0) | ((period >> 8) as u8 & 0x0F);
        pattern[cell + 1] = (period & 0xFF) as u8;
        pattern[cell + 2] = ((sample & 0x0F) << 4) | EFFECT_SET_VOLUME;
        pattern[cell + 3] = 64;
    }
    module.extend_from_slice(&pattern);

    // Uma senoide de 32 bytes com sinal, como o Amiga espera.
    let sine: Vec<u8> = (0..32)
        .map(|index| {
            let phase = std::f64::consts::TAU * f64::from(index) / 32.0;
            (phase.sin() * 127.0) as i8 as u8
        })
        .collect();
    module.extend_from_slice(&sine);
    module
}

#[test]
fn modulo_com_dados_reais_produz_audio() {
    let module = audible_module();
    let mut engine = player::load(&module, SAMPLE_RATE).expect("o motor deve carregar o módulo");
    assert!(engine.duration_seconds() > 0.0, "duração deve ser positiva");

    let mut out = std::io::Cursor::new(Vec::new());
    let rendered = offline::render_wav(engine.as_mut(), SAMPLE_RATE, &mut out)
        .expect("render em memória não falha");
    assert!(rendered.frames > 0, "nenhum quadro renderizado");

    let bytes = out.into_inner();
    let peak = bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
        .max()
        .expect("o WAV tem amostras");

    // O limiar existe para distinguir som de ruído numérico: um erro na cadeia entrega
    // silêncio absoluto, não um sinal fraco.
    assert!(
        peak > 1_000,
        "o render saiu praticamente silencioso (pico {peak})"
    );
}

#[test]
fn conteudo_invalido_nao_carrega_o_motor() {
    assert!(player::load(b"isto nao e um modulo", SAMPLE_RATE).is_err());
}

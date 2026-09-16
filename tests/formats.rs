//! Leitura de metadados dos quatro formatos, sobre módulos sintéticos mínimos.
//!
//! Os módulos são construídos byte a byte a partir da especificação de cada formato: dá para
//! afirmar exatamente o que deve sair, e o repositório não precisa carregar binários.

use xmcli::formats::{self, FormatError};
use xmcli::song::Dialect;

/// Escreve `bytes` em `offset`, crescendo o buffer com zeros quando preciso.
fn put(buffer: &mut Vec<u8>, offset: usize, bytes: &[u8]) {
    let end = offset + bytes.len();
    if buffer.len() < end {
        buffer.resize(end, 0);
    }
    buffer[offset..end].copy_from_slice(bytes);
}

fn put_u16_le(buffer: &mut Vec<u8>, offset: usize, value: u16) {
    put(buffer, offset, &value.to_le_bytes());
}

fn put_u32_le(buffer: &mut Vec<u8>, offset: usize, value: u32) {
    put(buffer, offset, &value.to_le_bytes());
}

fn protracker_module() -> Vec<u8> {
    let mut module = vec![0_u8; 1084];
    put(&mut module, 0, b"space debris");
    // Primeira amostra: nome e comprimento de 32 bytes (16 words, big-endian).
    put(&mut module, 20, b"kick drum");
    put(&mut module, 20 + 22, &16_u16.to_be_bytes());
    put(&mut module, 20 + 30, b"snare");
    put(&mut module, 20 + 30 + 22, &8_u16.to_be_bytes());
    module[950] = 2; // comprimento da sequência
    module[952] = 0; // ordem 0 -> padrão 0
    module[953] = 1; // ordem 1 -> padrão 1
    put(&mut module, 1080, b"M.K.");
    module
}

fn screamtracker_module() -> Vec<u8> {
    const INSTRUMENT_PARAGRAPH: u16 = 8;
    let instrument = usize::from(INSTRUMENT_PARAGRAPH) * 16;

    let mut module = vec![0_u8; 0x66];
    put(&mut module, 0, b"scream test");
    module[0x1C] = 0x1A;
    module[0x1D] = 16;
    put_u16_le(&mut module, 0x20, 2); // ordens
    put_u16_le(&mut module, 0x22, 1); // instrumentos
    put_u16_le(&mut module, 0x24, 1); // padrões
    put_u16_le(&mut module, 0x28, 0x1320); // Scream Tracker 3.20
    put(&mut module, 0x2C, b"SCRM");
    // Painel: quatro canais em uso, os demais sem uso.
    put(&mut module, 0x40, &[0, 1, 2, 3]);
    put(&mut module, 0x44, &[0xFF; 28]);
    put(&mut module, 0x60, &[0, 1]);
    put_u16_le(&mut module, 0x62, INSTRUMENT_PARAGRAPH);
    put_u16_le(&mut module, 0x64, INSTRUMENT_PARAGRAPH * 2);

    put(&mut module, instrument + 0x30, b"bass hit");
    put(&mut module, instrument + 0x4C, b"SCRS");
    module
}

fn fasttracker_module() -> Vec<u8> {
    const HEADER_SIZE: u32 = 276;
    const PATTERN_HEADER: u32 = 9;
    const INSTRUMENT_SIZE: u32 = 263;
    const SAMPLE_HEADER_SIZE: u32 = 40;
    const SAMPLE_BYTES: u32 = 16;

    let pattern = 0x3C + HEADER_SIZE as usize;
    let instrument = pattern + PATTERN_HEADER as usize;
    let sample_header = instrument + INSTRUMENT_SIZE as usize;

    let mut module = vec![0_u8; pattern];
    put(&mut module, 0, b"Extended Module: ");
    put(&mut module, 0x11, b"second reality");
    module[0x25] = 0x1A;
    put(&mut module, 0x26, b"FastTracker v2.00");
    put_u16_le(&mut module, 0x3A, 0x0104);
    put_u32_le(&mut module, 0x3C, HEADER_SIZE);
    put_u16_le(&mut module, 0x40, 2); // ordens
    put_u16_le(&mut module, 0x44, 8); // canais
    put_u16_le(&mut module, 0x46, 1); // padrões
    put_u16_le(&mut module, 0x48, 1); // instrumentos
    put_u16_le(&mut module, 0x4C, 6); // speed
    put_u16_le(&mut module, 0x4E, 125); // bpm

    put_u32_le(&mut module, pattern, PATTERN_HEADER);
    put_u16_le(&mut module, pattern + 5, 64); // linhas
    put_u16_le(&mut module, pattern + 7, 0); // padrão vazio

    put_u32_le(&mut module, instrument, INSTRUMENT_SIZE);
    put(&mut module, instrument + 4, b"lead synth");
    put_u16_le(&mut module, instrument + 27, 1); // uma amostra
    put_u32_le(&mut module, instrument + 29, SAMPLE_HEADER_SIZE);

    put_u32_le(&mut module, sample_header, SAMPLE_BYTES);
    put(&mut module, sample_header + 0x12, b"saw wave");
    module.resize(
        sample_header + SAMPLE_HEADER_SIZE as usize + SAMPLE_BYTES as usize,
        0,
    );
    module
}

fn impulsetracker_module() -> Vec<u8> {
    const INSTRUMENT: u32 = 0x100;
    const SAMPLE: u32 = 0x200;
    const MESSAGE: u32 = 0x300;
    const MESSAGE_TEXT: &[u8] = b"greetings to everyone\rsecond line";

    let mut module = vec![0_u8; 0xD0];
    put(&mut module, 0, b"IMPM");
    put(&mut module, 0x04, b"impulse test");
    put_u16_le(&mut module, 0x20, 3); // ordens
    put_u16_le(&mut module, 0x22, 1); // instrumentos
    put_u16_le(&mut module, 0x24, 1); // amostras
    put_u16_le(&mut module, 0x26, 1); // padrões
    put_u16_le(&mut module, 0x28, 0x0214); // Impulse Tracker 2.14
    put_u16_le(&mut module, 0x2C, 0x0004); // modo instrumento
    put_u16_le(&mut module, 0x36, MESSAGE_TEXT.len() as u16);
    put_u32_le(&mut module, 0x38, MESSAGE);
    // Oito canais ligados; os 56 restantes desligados (bit 7 no panorama).
    put(&mut module, 0x40, &[32; 8]);
    put(&mut module, 0x48, &[128 + 32; 56]);
    put(&mut module, 0xC0, &[0, 1, 255]);
    put_u32_le(&mut module, 0xC3, INSTRUMENT);
    put_u32_le(&mut module, 0xC7, SAMPLE);
    put_u32_le(&mut module, 0xCB, 0x400);

    put(&mut module, INSTRUMENT as usize, b"IMPI");
    put(&mut module, INSTRUMENT as usize + 0x20, b"church organ");
    put(&mut module, SAMPLE as usize, b"IMPS");
    put(&mut module, SAMPLE as usize + 0x14, b"organ c4");
    put(&mut module, MESSAGE as usize, MESSAGE_TEXT);
    module
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("mod", protracker_module()),
        ("s3m", screamtracker_module()),
        ("xm", fasttracker_module()),
        ("it", impulsetracker_module()),
    ]
}

#[test]
fn mod_e_lido() {
    let song = formats::read(&protracker_module()).expect("módulo válido");
    assert_eq!(song.dialect, Dialect::ProTracker);
    assert_eq!(song.title, "space debris");
    assert_eq!(song.channels, 4);
    assert_eq!(song.orders, 2);
    assert_eq!(song.patterns, 2);
    assert_eq!(song.samples.len(), 31);
    assert_eq!(song.samples[0], "kick drum");
    assert_eq!(song.samples[1], "snare");
}

#[test]
fn s3m_e_lido() {
    let song = formats::read(&screamtracker_module()).expect("módulo válido");
    assert_eq!(song.dialect, Dialect::ScreamTracker3);
    assert_eq!(song.title, "scream test");
    assert_eq!(song.channels, 4);
    assert_eq!(song.orders, 2);
    assert_eq!(song.patterns, 1);
    assert_eq!(song.samples, vec!["bass hit"]);
    assert_eq!(song.tracker.as_deref(), Some("Scream Tracker 3.20"));
}

#[test]
fn xm_e_lido() {
    let song = formats::read(&fasttracker_module()).expect("módulo válido");
    assert_eq!(song.dialect, Dialect::FastTracker2);
    assert_eq!(song.title, "second reality");
    assert_eq!(song.channels, 8);
    assert_eq!(song.orders, 2);
    assert_eq!(song.patterns, 1);
    assert_eq!(song.instruments, vec!["lead synth"]);
    assert_eq!(song.samples, vec!["saw wave"]);
    assert_eq!(song.tracker.as_deref(), Some("FastTracker v2.00"));
}

#[test]
fn it_e_lido() {
    let song = formats::read(&impulsetracker_module()).expect("módulo válido");
    assert_eq!(song.dialect, Dialect::ImpulseTracker);
    assert_eq!(song.title, "impulse test");
    assert_eq!(song.channels, 8);
    assert_eq!(song.orders, 3);
    assert_eq!(song.patterns, 1);
    assert_eq!(song.instruments, vec!["church organ"]);
    assert_eq!(song.samples, vec!["organ c4"]);
    assert_eq!(song.tracker.as_deref(), Some("Impulse Tracker 2.14"));
    assert_eq!(
        song.message.as_deref(),
        Some("greetings to everyone\nsecond line"),
    );
}

#[test]
fn deteccao_ignora_a_extensao_e_olha_o_conteudo() {
    for (name, bytes) in fixtures() {
        let detected = formats::detect(&bytes).expect("fixture deve ser reconhecida");
        assert_eq!(detected.extension(), name);
    }
}

#[test]
fn conteudo_qualquer_e_recusado() {
    let error = formats::read(b"isto nao e um modulo de tracker, e so texto").expect_err("recusa");
    assert!(matches!(error, FormatError::Unsupported), "obtido: {error}");
    assert!(formats::read(b"").is_err());
}

#[test]
fn truncamento_em_qualquer_ponto_nao_entra_em_panico() {
    for (name, bytes) in fixtures() {
        for length in 0..bytes.len() {
            // O que importa é não haver pânico: um prefixo pode até ser um módulo válido.
            let _ = formats::read(&bytes[..length]);
        }
        // E o arquivo inteiro continua legível depois do laço.
        assert!(
            formats::read(&bytes).is_ok(),
            "{name} deixou de ser legível"
        );
    }
}

#[test]
fn corrupcao_de_bytes_nao_entra_em_panico() {
    // Percorre o arquivo em passos primos para cobrir cabeçalho, tabelas e ponteiros sem
    // gastar o tempo de um fuzzing completo — esse roda separado, em fuzz/.
    const STEP: usize = 7;
    const PATTERNS: [u8; 4] = [0x00, 0x01, 0x7F, 0xFF];

    for (_, bytes) in fixtures() {
        for offset in (0..bytes.len()).step_by(STEP) {
            for pattern in PATTERNS {
                let mut damaged = bytes.clone();
                damaged[offset] = pattern;
                let _ = formats::read(&damaged);
            }
        }
    }
}

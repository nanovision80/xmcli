//! Joga bytes arbitrários nos descompressores de contêiner (RF-104, RNF-09).
//!
//! Cobre também as bombas de descompressão: o teto precisa segurar qualquer entrada.

#![no_main]

/// Teto de saída usado no fuzzing; pequeno de propósito, para exercitar o corte.
const LIMIT: u64 = 1 << 20;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = xmcli::io::container::unpack(data.to_vec(), LIMIT);
});

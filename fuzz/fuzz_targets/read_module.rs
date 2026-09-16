//! Joga bytes arbitrários nos loaders (RNF-09).
//!
//! O contrato é simples: qualquer entrada devolve `Ok` ou `Err`, nunca pânico, nunca leitura
//! fora dos limites, nunca laço infinito.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = xmcli::formats::read(data);
});

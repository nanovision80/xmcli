# Fuzzing

Alvos de fuzzing dos loaders e dos contêineres (RNF-09).

```console
$ cargo install cargo-fuzz
$ cargo +nightly fuzz run read_module
$ cargo +nightly fuzz run unpack_container
```

O corpus semente vive em `corpus/<alvo>/`. Os módulos sintéticos usados em `tests/formats.rs`
são bons pontos de partida: eles exercitam cabeçalho, tabelas de ponteiro e nomes.

O CI não roda estes alvos — eles exigem nightly e tempo. O que roda a cada commit é a
varredura determinística de truncamento e corrupção em `tests/formats.rs`, que cobre a mesma
classe de falha em segundos.

# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
o projeto segue [versionamento semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

## [0.1.0] — 2026-09-16

Primeira versão publicada. Fecha a fase **F0** do roteiro (etapas 0 a 2): o player
reproduz corretamente pelo motor externo, ainda sem interface.

### Adicionado

- Fonte única da verdade em `config/defaults.json`, com as structs `serde` de
  `src/config/settings.rs` como schema e validação em tempo de compilação pelo `build.rs`
  (RF-706).
- Mesclagem de configuração em `Settings` congelada, na precedência CLI > env > usuário >
  padrão (RF-706).
- CLI com `clap`, `--help` completo e códigos de saída estáveis (RF-707); `--info` em texto
  e em JSON (RF-703).
- Leitura de arquivo, de `stdin` e de diretório recursivo, com teto de tamanho
  (RF-105, RF-701).
- Detecção de formato por conteúdo, nunca por extensão (RF-101).
- Contêineres `.zip`, `.gz`, `.xz`, `.bz2` e `PP20`, aninhados até 4 camadas, com teto
  contra bomba de descompressão (RF-104).
- Transcodificação CP437 → UTF-8 preservando a arte ASCII (RF-150).
- Modelo `Song` com metadados e o enum `Dialect` (RF-201).
- Erro descritivo para módulo malformado, com o que se lia, o offset e o tamanho real
  (RF-102).
- Saída por `cpal` com seleção de dispositivo, anel SPSC e contagem de estouro
  (RF-401, RF-402); relógio por quadros efetivamente entregues (RF-406).
- Trait `Engine` com o `libopenmpt` como primeira implementação, mais o backend nulo para
  medição determinística sem dispositivo (RF-405).
- Render offline para WAV de 16 bits (RF-403) e PCM cru `f32` em `stdout` (RF-404).
- Alvos de fuzzing para loaders e contêineres, e varredura determinística de truncamento e
  corrupção que roda no CI sem nightly (RNF-09).
- CI em matriz Linux, macOS e Windows: `fmt`, `clippy -D warnings`, testes, build de release
  e um job que guarda a MSRV 1.85.

### Notas

- O Windows compila com `--no-default-features`: o `openmpt-sys` apenas linka contra a
  libopenmpt do sistema, que não tem pacote pronto no runner. A etapa 8 do roteiro traz o
  motor próprio e remove essa assimetria.
- A reprodução por dispositivo real ainda não foi exercitada em hardware de áudio; o teste de
  resistência de 30 minutos sem xrun está previsto para a etapa 11.
- O descompressor `PP20` foi verificado apenas contra fluxos construídos a partir da descrição
  do formato, ainda não contra um arquivo real.

[Não lançado]: https://github.com/nanovision80/xmcli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nanovision80/xmcli/releases/tag/v0.1.0

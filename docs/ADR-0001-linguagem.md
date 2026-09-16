# ADR-0001 — Linguagem e stack do xmcli

- **Status:** proposto
- **Contexto:** player de módulos tracker (`.mod`/`.s3m`/`.xm`/`.it`) para terminal, com
  interface ASCII colorida e animada. Ver `docs/REQUISITOS.md`.

## Forças em jogo

1. **Tempo real duro no áudio.** O callback do dispositivo tem orçamento de poucos
   milissegundos. Qualquer pausa de GC, alocação imprevisível ou lock produz *underrun*
   audível. Isso elimina Go, C#, Java, Python.
2. **Entrada hostil.** Módulos vêm de coleções da internet, muitos truncados ou corrompidos.
   Loaders de tracker são um histórico conhecido de estouros de buffer. Segurança de memória
   no parser não é luxo.
3. **Interface de terminal de alto desempenho.** A UI é um player estilo Winamp cujo elemento
   dominante é um *palco de visualização* (espectro, osciloscópio, barras flutuantes, efeitos
   generativos estilo MilkDrop). Isso significa um laço numérico de ponto flutuante sobre um
   framebuffer, a 60 fps, mais quantização para células do terminal e coalescência de sequências
   de cor. Não é "desenhar texto" — é rasterização.
4. **Distribuição.** Binário único, sem runtime, para Linux/macOS/Windows.
5. **FFI barato** com `libopenmpt`/`libxmp`, para usar um motor de referência na fase inicial
   e como oráculo de regressão depois.

## Decisão

**Rust.**

É a única linguagem que satisfaz (1), (2) e (3) ao mesmo tempo: performance e previsibilidade
de C/C++ sem GC, com segurança de memória verificada em compilação exatamente onde o risco
está concentrado (os loaders binários), e um laço numérico que auto-vetoriza bem para o palco
de visualização. Soma a isso `ratatui`/`crossterm` para a TUI, `cpal` para áudio nativo
multiplataforma, FFI de custo zero com C, e binário estático com cross-compile simples.

**Stack:** `cpal` · `ratatui` + `crossterm` · `clap` · `serde`/`serde_json` · `tracing` ·
`realfft` · `glam` · `noise` · `palette` · `ringbuf`/`triple_buffer` · `codepage-437` ·
`cargo-fuzz` · `criterion`. O palco tem camada própria de emissão de células com coalescência
de SGR — o render por diff genérico não serve quando quase toda célula muda a cada quadro.

**Motor:** fase 1 via FFI com `libopenmpt` (precisão imediata); fase posterior substitui por
motor próprio em Rust, validado contra o libopenmpt como oráculo. A UI estilo Winamp **aumenta**
o valor dessa substituição: o palco vive de PCM por canal (RF-308) e de eventos de disparo de
nota com precisão de amostra (RF-210/RF-538), que nenhuma das bibliotecas prontas expõe.

## Alternativas consideradas

- **C++17/20 + libopenmpt + FTXUI** — vice-líder legítimo, e a escolha correta se a equipe já
  for de C++: acesso direto ao motor de referência, sem camada de FFI. Rejeitado por
  (2) e pelo custo de build multiplataforma.
- **C + ncurses** — máxima portabilidade, pior segurança, TUI trabalhosa. Rejeitado.
- **Zig** — modelo de execução adequado, ecossistema de TUI e de áudio ainda imaturo para
  este escopo. Reavaliar no futuro.
- **Go** — rejeitado: GC e o custo de travessia cgo dentro do callback de áudio.
- **Python (textual/rich)** — GIL e latência inviabilizam o caminho de áudio. Útil apenas
  para ferramentas auxiliares e análise do corpus de testes.

## Consequências

- Curva de aprendizado maior se a equipe não conhecer Rust, especialmente para código
  *lock-free* na fronteira UI ↔ áudio.
- Dependência de build em C++ enquanto o `libopenmpt` estiver vinculado (some na fase 3).
- Ganho: fuzzing dos loaders integrado ao CI desde o primeiro dia, um único artefato por
  plataforma sem runtime, e um motor de visualização determinístico e testável por snapshot.

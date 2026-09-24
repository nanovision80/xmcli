# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
o projeto segue [versionamento semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

Fecha a etapa 3 do roteiro: o barramento de visualização que toda a interface vai consumir,
com o erro de sincronismo medido antes de existir a primeira tela.

### Adicionado

- Filas SPSC sem lock e sem alocação depois da partida, com o quadro em que cada mensagem
  ocorre carimbado nela. Fila cheia descarta a mensagem nova e conta a perda, para a interface
  se ressincronizar (etapa 3.1).
- Barramento master pós-mix em blocos fixos de 256 quadros, com o histórico configurável em
  `bus.master_history_ms` (RF-309).
- Snapshot de estado por bloco de render: order, padrão, linha, speed, BPM e nível por canal,
  com a profundidade da fila em `bus.snapshot_capacity` (RF-521).
- Compensação de latência do dispositivo: o relógio publica o instante em que o quadro 0 soou
  e a posição audível interpola entre uma entrega e a seguinte (RF-620). Contra um
  dispositivo simulado, o erro é de 0,04 ms com a máquina quieta e 0,08 ms com metade dos
  processadores ocupados, contra 20 ms sem compensação e um orçamento de 16 ms.
- Início da camada de terminal: modo raw, tela alternativa e cursor oculto (RF-515), e o
  terminal restaurado na saída normal, em pânico — inclusive com `panic = "abort"` — e em
  `SIGINT`, `SIGTERM` e `SIGHUP` (RF-516, RNF-10). Ainda sem uso pelo player, que segue sem
  interface.
- Detecção da profundidade de cor do terminal — sem cor, 16, 256 ou truecolor — por `TERM`,
  `COLORTERM` e, no Windows Terminal, `WT_SESSION`, com `--mono` e `NO_COLOR` desligando a
  cor (RF-514). Por enquanto só aparece no log com `-v`.
- Grade de células e pintura de quadros: só as células que mudaram vão para o terminal, a
  sequência de cor só sai quando muda — inclusive de um quadro para o outro — e o quadro
  inteiro vai num `write` só (RF-515, RF-604, RF-621). A comparação é feita depois de converter
  a cor para o terminal, e caracteres de controle vindos de módulo não chegam a ele.
- Ritmo de quadros adaptativo: o throughput do terminal é medido pelo tempo do `write`, e o
  intervalo entre quadros respeita o menor entre ele e o teto de 400 KB/s, de 30 a 60 fps
  (RF-601, RF-622, RNF-07). Quadro acima do orçamento desce a cor de truecolor para 256 e
  para 16; quadros dentro dele a devolvem (RF-621). Nova seção `ui` na configuração e nova
  opção `--max-frame-bytes`.
- Detecção de mudança de tamanho do terminal, pelo `SIGWINCH` no Unix e pelos eventos do
  console no Windows: a rajada de avisos de um arrasto de janela vira uma resposta só, com o
  tamanho real, e a tela seguinte é repintada inteira (RF-513).
- Oito módulos de amostra de licença pública nos quatro formatos, em `samples/`.

### Corrigido

- O fim da música não conta mais como estouro do dispositivo. Tocar um módulo até o fim
  terminava com o aviso errado de elevar `audio.latency_ms` (RF-402).

### Notas

- Tick, nota e instrumento por canal ainda não entram no snapshot: o libopenmpt não os expõe.
  Chegam com o motor próprio da etapa 8.
- O sincronismo foi medido contra um dispositivo simulado. A medição contra hardware real
  está prevista para a etapa 11.
- No Windows, só a saída normal e o pânico restauram o terminal: lá não existem os sinais do
  RF-516, e em modo raw o `Ctrl+C` chega como tecla.

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

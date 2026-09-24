# CLAUDE.md — orientações de desenvolvimento do xmcli

Guia para qualquer pessoa (ou agente) que escreva código neste repositório.
**Leia antes de abrir o primeiro arquivo.**

## 1. Contexto

`xmcli` é um player de módulos tracker (`.mod`, `.s3m`, `.xm`, `.it`) para terminal, escrito em
**Rust**, com interface estilo Winamp em ASCII colorido e um palco de visualização animado.

| Documento | Papel |
|---|---|
| [`docs/REQUISITOS.md`](docs/REQUISITOS.md) | **Fonte da verdade do escopo.** Todo código implementa um RF/RNF identificado. Não há código sem requisito. |
| [`docs/ADR-0001-linguagem.md`](docs/ADR-0001-linguagem.md) | Decisão de linguagem e stack |
| `config/*.json` | **Fonte da verdade dos valores.** Ver §4. |

Se uma mudança precisa de comportamento que não está em `docs/REQUISITOS.md`, o requisito entra
no documento primeiro. Código antes de requisito é como se cria escopo fantasma.

**O plano de execução completo, em ordem, está na [§12](#12-roteiro-de-execução--do-zero-a-100-).**
Comece por lá para saber o que fazer em seguida.

---

## 2. Invariantes de arquitetura (não negociáveis)

Estas regras vêm de restrições físicas do problema, não de gosto. Violá-las produz defeito
audível ou visível.

1. **O callback de áudio não aloca, não trava, não faz I/O e não entra em pânico.**
   Sem `Vec::push`, sem `String`, sem `Mutex`, sem `println!`, sem leitura de arquivo, sem
   `unwrap()` que possa falhar. Tudo que ele usa é pré-alocado e passado por referência.
2. **A UI perde quadros; o áudio nunca perde amostras.** Em disputa por CPU, a visualização
   degrada (resolução, fps, cor) — jamais o contrário.
3. **Comunicação áudio ↔ UI só por fila SPSC sem lock**, em uma direção por fila. Nada de estado
   compartilhado mutável entre os dois threads.
4. **Tudo que a UI mostra é carimbado com a amostra em que ocorre**, e ela consome o que
   corresponde ao *tempo audível* (posição mixada − buffer do dispositivo). Ver RF-620.
5. **Os dialetos de tracker não são normalizados.** Os *quirks* de ProTracker/ST3/FT2/IT são
   comportamento correto, não bug. Nunca "conserte" um quirk para unificar formatos.
6. **Todo arquivo de entrada é hostil.** Nenhum loader confia em tamanho, offset ou contagem
   declarada no cabeçalho. Índice sempre validado contra o tamanho real. Sem `unsafe` em loader.
7. **Configuração é resolvida uma vez, na inicialização, para uma estrutura congelada.**
   Nenhum laço quente lê arquivo, mapa ou string de configuração.

---

## 3. Princípios de código

Aplicados como regras verificáveis, não como slogans.

### KISS — a solução mais simples que atende ao requisito
- Prefira **função livre** a método; **struct de dados** a trait; **`match`** a hierarquia.
- Uma função faz uma coisa e cabe na tela. Se precisa de comentário para explicar as seções,
  são funções separadas.
- Aninhamento máximo de 3 níveis. Use *early return*.
- Nada de "engenharia para o futuro". O futuro chega diferente do que você imaginou.

### YAGNI — não implemente o que ninguém pediu
Não crie:
- trait com **uma única implementação** (exceção: fronteiras que os testes realmente
  substituem, como o backend de áudio, que tem um backend nulo real);
- *builder* para menos de 4 campos opcionais;
- getter/setter trivial — em struct de dados, o campo é público;
- `new()` que apenas monta campos públicos quando `Struct { .. }` já resolve;
- wrapper que só encaminha chamadas;
- módulo `utils`, `helpers`, `common`, `manager`, `base` — são nomes que significam
  "não decidi onde isto mora";
- parâmetro, flag ou variante de enum sem chamador hoje;
- abstração para "suportar outros formatos no futuro". Temos quatro, e eles estão especificados.

### DRY — conhecimento em um lugar só
- Uma regra de negócio existe em **exatamente um** lugar. Fórmula de tempo por tick, rampa de
  caracteres ASCII, cor determinística por instrumento, conversão CP437 → UTF-8: uma
  implementação cada, em módulo nomeado.
- Na **segunda** vez que copiar o mesmo trecho, extraia. Na primeira, espere — duplicação
  aparente às vezes é coincidência, e abstrair cedo demais custa mais que duplicar.
- DRY é sobre *conhecimento*, não sobre caracteres iguais. Duas funções que por acaso têm o
  mesmo corpo mas mudam por razões diferentes **devem** permanecer separadas.

### SOLID — na dose que uma aplicação em Rust pede
- **SRP**: um módulo tem um motivo para mudar. `formats/xm/` muda quando a especificação do XM
  muda; `ui/stage/spectrum.rs` muda quando a visualização de espectro muda. Se um arquivo muda
  por dois motivos, são dois arquivos.
- **OCP**: extensão por **dados**, não por herança. Um novo preset de visualização é um arquivo
  JSON, não uma classe. Um novo tema é uma entrada em `palettes.json`.
- **LSP/ISP**: traits pequenos e focados; nenhum implementador deve precisar de `todo!()` ou
  `unimplemented!()` em um método que não faz sentido para ele.
- **DIP**: o núcleo (replayer, mixer, palco) não conhece `cpal`, `crossterm` nem o sistema de
  arquivos. Dependências concretas entram pelas bordas (`audio/`, `ui/term/`, `io/`).

---

## 4. Fonte única da verdade: `config/*.json`

**Nenhuma constante sintonizável no meio do código.** Valores de configuração vivem em JSON e
são materializados em tipos Rust na inicialização.

```
config/
  defaults.json        # padrões de runtime: áudio, UI, palco, limites, orçamentos
  keymap.json          # teclas -> ações            (entra na etapa 5)
  palettes.json        # temas de cor               (entra na etapa 5)
  presets/*.json       # presets de visualização    (entra na etapa 7)
```

**O schema são as structs `serde` de `src/config/settings.rs`**, não um arquivo `.schema.json`
paralelo — um schema separado seria o mesmo conhecimento em dois lugares, e um dia divergiriam.
`#[serde(deny_unknown_fields)]` recusa chave desconhecida, os tipos recusam valor de tipo
errado e `Settings::validate` recusa valor fora de faixa. `build.rs` inclui esse mesmo arquivo e
roda as três verificações sobre `defaults.json` **em tempo de compilação**: um padrão inválido
quebra o build, não a execução.

**Fluxo:** `config/*.json` → validação por schema → merge com
`$XDG_CONFIG_HOME/xmcli/config.json` do usuário → merge com env → merge com CLI →
**struct `Settings` congelada**, passada por referência. Precedência: **CLI > env > usuário > padrão**.

### O que vai no JSON e o que não vai

| Vai no JSON | Não vai no JSON |
|---|---|
| Padrões de áudio (taxa, buffer, latência alvo) | Tabelas de período do ProTracker |
| Orçamentos: fps alvo, bytes por quadro, teto de CPU | Clamp de período 113–856 do ProTracker |
| Rampas de caracteres ASCII, paletas, temas | O fator `2.5` da fórmula de tempo por tick |
| Teclas, parâmetros e ligações dos presets | Layout binário dos cabeçalhos de formato |
| Limites de canais, tamanhos de ring buffer | Semântica dos efeitos de cada tracker |

A coluna da direita vem da **especificação dos formatos**. Não é sintonizável: expor no JSON
convidaria alguém a "ajustar" e quebrar a fidelidade de reprodução. Mas continua **proibido**
aparecer como literal solto — mora em `src/formats/<fmt>/spec.rs`, como constante nomeada, com
a referência da especificação no comentário.

### Números mágicos

```rust
// ERRADO — de onde vem 2.5? e por que 125?
let samples_per_tick = (sample_rate as f64 * 2.5) / 125.0;

// CERTO
/// Fator da fórmula de tempo dos trackers: um "tick" dura 2.5/BPM segundos.
/// Vem da temporização original do Amiga; é constante em MOD/S3M/XM/IT.
const TICK_SECONDS_NUMERATOR: f64 = 2.5;

fn samples_per_tick(sample_rate: u32, bpm: u16) -> f64 {
    f64::from(sample_rate) * TICK_SECONDS_NUMERATOR / f64::from(bpm)
}
```

Regra prática: **qualquer literal numérico que não seja `0`, `1` ou `-1` precisa de nome.**
Se o nome for sintonizável, vai para o JSON. Se vier de especificação, vira `const` documentada.
O mesmo vale para strings: caminhos, rótulos, sequências de escape e a rampa ASCII são dados
nomeados, nunca literais no meio da lógica.

### Constante espalhada

```rust
// ERRADO — o mesmo conhecimento em três arquivos; um dia mudam dois.
// ui/stage/bars.rs      const RAMP: &str = " .:-=+*#%@";
// ui/stage/generative.rs const CHARS: &str = " .:-=+*#%@";
// ui/stage/spectrum.rs  let ramp = " .:-=+*#%@";

// CERTO — um lugar, vindo do JSON, resolvido uma vez.
// config/defaults.json  ->  settings.stage.ascii_ramp
```

---

## 5. Estrutura do repositório

```
config/           fonte da verdade dos valores (§4)
build.rs          gera constantes tipadas a partir de config/, valida schemas
src/
  main.rs         apenas: parse de CLI, montagem, execução. Sem lógica.
  cli/            definição de argumentos e mapeamento para Settings
  config/         carga, merge, validação -> Settings congelado
  formats/        loaders por formato; cada um com spec.rs (constantes da especificação)
  song/           modelo intermediário + Dialect (comportamento por tracker)
  player/         replayer por tick, canais virtuais, emissão de eventos musicais
  mixer/          DSP: interpolação, ramping, filtros, barramentos de saída
  audio/          backends de dispositivo, relógio, compensação de latência
  bus/            filas SPSC: PCM master, PCM por canal, snapshot, eventos
  ui/
    chrome/       marquee, transporte, seek, status, playlist, mixer de canais
    stage/        palco: spectrum, scope, bars, generative, quantize
    term/         capacidades, coalescência de SGR, escrita, resize
tests/            integração e conformidade
fuzz/             alvos de fuzzing por loader
benches/          orçamentos de CPU e de bytes por quadro
```

`main.rs` monta e sai do caminho. Se ele cresce, a lógica está no lugar errado.

---

## 6. Erros, logs e comentários

- **Erros:** `thiserror` para tipos de erro de biblioteca, `anyhow` só na borda do binário.
  Nada de `unwrap()`/`expect()` fora de testes e de invariantes provadas — e quando usar
  `expect()`, a mensagem explica **por que não pode falhar**.
- **Um módulo malformado é um erro esperado, não um pânico.** Mensagem diz o arquivo, o offset
  e o que se esperava encontrar.
- **Logs** vão para `stderr` via `tracing`, nunca para a TUI. Nenhum log no callback de áudio.
- **Comentários explicam por quê, não o quê.** O código já diz o que faz. Comentário obrigatório
  em toda implementação de *quirk* de tracker, citando a fonte — sem isso, alguém "conserta" o
  bug e quebra a reprodução:

```rust
// ProTracker: 9xx com offset além do fim da amostra reinicia do zero em vez de silenciar.
// Vários módulos dependem disso para o efeito de "corte". Não normalizar.
```

---

## 7. Testes

| Tipo | Regra |
|---|---|
| Conformidade | Cada quirk implementado tem um caso no corpus de conformidade |
| Regressão de render | Render offline determinístico, comparado com referência |
| Fuzzing | Todo loader tem alvo em `fuzz/`; CI falha em qualquer crash ou timeout |
| Visualização | Motor determinístico dada (semente, entrada) → snapshot do framebuffer por hash |
| UI | Snapshot do buffer do terminal em 80×24 / 120×40 / 200×60 e em 16/256/truecolor |
| Orçamento | Benchmark de tempo/quadro e bytes/quadro; regressão de desempenho falha o CI |

Teste comportamento observável, não estrutura interna. Um teste que quebra ao renomear um campo
privado está testando a coisa errada.

---

## 8. Comandos

```console
$ cargo fmt --all                       # formatação, sem discussão
$ cargo clippy --all-targets -- -D warnings
$ cargo test --all
$ cargo bench                           # orçamentos de CPU e de banda
$ cargo fuzz run <alvo>                 # loaders
$ cargo run -- --info --json <modulo>   # inspeção rápida
```

`fmt` e `clippy` limpos são condição de entrada, não sugestão.

---

## 9. Dependências

Cada dependência nova precisa de justificativa no PR: o que resolve, por que não vale escrever,
e qual o custo em tempo de compilação e em superfície de manutenção. A stack aprovada está no
[ADR-0001](docs/ADR-0001-linguagem.md); sair dela é uma decisão, não um detalhe.

---

## 10. Commits e PRs

- Mensagem no imperativo, explicando **por quê**, não repetindo o diff.
- Um PR resolve uma coisa. Refatoração e mudança de comportamento não viajam juntas.
- O PR cita os RF/RNF que implementa.

---

## 11. Definition of done

Antes de abrir o PR, tudo abaixo é verdade:

- [ ] Implementa um RF/RNF identificado em `docs/REQUISITOS.md`
- [ ] `cargo fmt` e `cargo clippy -D warnings` limpos
- [ ] Testes passando; comportamento novo tem teste novo
- [ ] Nenhum literal numérico ou textual sem nome
- [ ] Nenhum valor sintonizável fora de `config/*.json`
- [ ] Nenhuma abstração sem pelo menos um uso real hoje
- [ ] Nada novo no callback de áudio que aloque, trave, faça I/O ou entre em pânico
- [ ] Quirks implementados estão comentados com a fonte
- [ ] Se o escopo mudou, `docs/REQUISITOS.md` mudou junto

---

## 12. Roteiro de execução — do zero a 100 %

Ordem **sequencial e por dependência**: cada etapa só começa quando a anterior fecha, porque
cada uma consome algo que a anterior produz. Onde há paralelismo seguro, está anotado.

Uma etapa está concluída quando **todos os seus itens estão marcados, os RF correspondentes têm
teste, e o CI está verde** — não quando "funciona na minha máquina".

| Etapa | Fase | Entrega |
|---|---|---|
| 0–2 | **F0** | Toca corretamente, sem interface |
| 3–5 | **F1** | Player estilo Winamp usável |
| 6 | **F2** | Palco com visualizações determinísticas |
| 7 | **F3** | Motor generativo com presets |
| 8–9 | **F4** | Motor de reprodução próprio e UI plena |
| 10–12 | **F5** | Periferia, endurecimento e release |

---

### Etapa 0 — Fundação do repositório
*Depende de: nada. Produz: projeto compilável, CI e a fonte única da verdade.*

- [x] 0.1 `cargo init` como binário; edition 2024, MSRV 1.85 em `Cargo.toml`, toolchain de
      desenvolvimento em `rust-toolchain.toml`
- [x] 0.2 `rustfmt.toml`, `.gitignore`, `.editorconfig`
      — *`clippy.toml` dispensado: a MSRV que ele carregaria já é `rust-version` no
      `Cargo.toml`, de onde o clippy a lê. Duas cópias divergiriam.*
- [x] 0.3 CI (GitHub Actions): `build`, `fmt`, `clippy -D warnings`, `test`, em matriz
      Linux/macOS/Windows
- [x] 0.4 `config/defaults.json` + as structs `serde` que o definem
      — *`keymap.json` e `palettes.json` adiados para a etapa 5, quando têm consumidor:
      chave de configuração sem leitor é peso morto (§3, YAGNI).*
- [x] 0.5 `build.rs` valida `defaults.json` em tempo de compilação (formato, chaves e faixas)
- [x] 0.6 `src/config/`: mesclagem das camadas e **`Settings` congelada**; testes de
      precedência CLI > env > usuário > padrão (RF-706)
- [x] 0.7 `src/cli/` com `clap`; `--help` completo; códigos de saída estáveis (RF-707)
- [x] 0.8 Erros (`thiserror`/`anyhow`) e `tracing` para `stderr` (RF-709)

> **Marco atingido.** `cargo run -- --help` funciona; `fmt`, `clippy -D warnings` e os 12 testes
> passam. A fonte da verdade existe antes da primeira linha de lógica — depois dela, extrair
> constantes espalhadas vira retrabalho.

### Etapa 1 — Entrada, formato e metadados *(sem áudio)*
*Depende de: 0. Produz: qualquer arquivo lido com segurança e descrito.*

- [x] 1.1 `src/io/`: leitura de arquivo e de `stdin` com teto de tamanho, e varredura
      recursiva de diretório (RF-105, RF-701)
- [x] 1.2 Detecção de formato **por conteúdo**, nunca por extensão (RF-101)
- [x] 1.3 Contêineres: `.zip`, `.gz`, `.xz`, `.bz2` e `PP20`, aninhados até 4 camadas, com
      teto contra bomba de descompressão (RF-104)
      — *o descompressor `PP20` só foi verificado contra fluxos construídos a partir da
      descrição do formato; falta confrontá-lo com um arquivo real (ver etapa 11).*
- [x] 1.4 Transcodificação CP437 → UTF-8 preservando a arte ASCII, inclusive os glifos de
      controle usados como desenho (RF-150)
- [x] 1.5 Modelo `Song` com os metadados + enum `Dialect` (RF-201)
- [x] 1.6 `--info` em texto e `--json` (RF-703)
- [x] 1.7 `fuzz/` com alvos para os loaders e para os contêineres, mais uma varredura
      determinística de truncamento e corrupção que roda no CI sem nightly (RNF-09)
- [x] 1.8 Erro descritivo para arquivo malformado: o que se lia, o offset e o tamanho real
      do arquivo (RF-102)

> **Marco atingido.** `xmcli --info` descreve os quatro formatos, abre contêiner com extensão
> mentirosa, lê de `stdin` e de diretório recursivo, e não quebra com arquivo truncado em
> nenhum ponto nem com bytes corrompidos.

### Etapa 2 — Áudio tocando com motor externo
*Depende de: 1. Produz: som correto, sem interface. **Fecha a F0.***

- [x] 2.1 `src/audio/device`: `cpal`, seleção de dispositivo por nome com queda para o padrão,
      anel SPSC entre a linha alimentadora e o callback, contagem de estouro (RF-401, RF-402)
- [x] 2.2 **Relógio por quadros efetivamente entregues ao dispositivo** (RF-406)
- [x] 2.3 Trait `Engine` com o `libopenmpt` como primeira implementação (§6, fase 1)
- [x] 2.4 Backend nulo: a renderização offline para um destino em memória dá medição
      determinística sem dispositivo nenhum (RF-405)
- [x] 2.5 Render offline para WAV de 16 bits (RF-403) e PCM cru `f32` em `stdout` (RF-404)
- [ ] 2.6 Fila SPSC de comandos UI → áudio: play/pause, seek, volume
      — *adiado para a etapa 5: o único remetente possível é a interface, que ainda não
      existe. Fila sem remetente é código morto (§3, YAGNI). O anel de áudio e o relógio,
      que têm consumidor hoje, foram feitos.*
- [x] 2.7 Auditoria de licença: `libopenmpt` 0.7.3 é BSD-3-Clause e as crates `openmpt` e
      `openmpt-sys` são BSD-3-Clause-Attribution. A atribuição está no README (RNF-12)

> **Marco (F0) atingido.** Renderiza os quatro formatos pelo motor externo; um MOD com amostra
> e padrão reais sai com pico de 14742 em 16 bits, não silêncio. A reprodução pelo dispositivo
> não pôde ser exercitada no contêiner de desenvolvimento (sem hardware de áudio): o caminho
> falha com mensagem clara e precisa ser confrontado com um dispositivo real — as 30 horas
> contínuas sem xrun ficam pendentes para a etapa 11.
>
> O trait `Engine` é uma das poucas abstrações justificadas: tem duas implementações reais
> hoje (libopenmpt e o gerador de teste que exercita a cadeia sem a biblioteca C) e uma
> terceira planejada na etapa 8.

### Etapa 3 — Barramento de visualização
*Depende de: 2. Produz: os dados que **toda** a interface consome.*

- [x] 3.1 `src/bus/`: filas SPSC sem lock, tipos de mensagem, **carimbo de amostra** em tudo
      — *os tipos de mensagem entram junto com quem os produz: o PCM master em 3.2, o
      snapshot de estado em 3.3. Aqui fica só a primitiva que todos usam.*
- [x] 3.2 Barramento master pós-mix (RF-309)
      — *a derivação para o laço de render entra na 3.4, junto com a compensação de
      latência: é ela que traz o primeiro consumidor. Aqui ficam o bloco, o fatiamento
      carimbado e a capacidade vinda de `bus.master_history_ms`.*
- [x] 3.3 Snapshot de estado por tick: order/row/tick/BPM/speed e estado por canal
      — *com o motor externo o estado é amostrado por bloco de render, não por tick, e o
      estado por canal é o VU: o libopenmpt não expõe tick, nota nem instrumento por
      canal (§6). Esses campos entram na etapa 8, com o motor que os conhece.*
- [x] 3.4 **Compensação de latência** (RF-620) com teste automatizado de erro de sincronismo
      — *o relógio publica uma **âncora**: o instante em que o quadro 0 soou. A posição
      audível é a reta que sai dela, e cada entrega ao dispositivo a recorrige, de modo que
      a imagem segue o ritmo real do hardware em vez de um ritmo suposto. Guardar a âncora,
      e não o par (instante, contador) da última entrega, dispensa ler dois valores de uma
      vez: um par lido pela metade erra exatamente um buffer — o defeito que a compensação
      existe para eliminar. Medido em `tests/sync.rs`: 0,03 ms contra 20 ms sem compensação.*
- [x] 3.5 Consumidor de teste que valida erro < 16 ms sob carga
      — *o consumidor segue o barramento master no tempo audível: guarda o bloco que contém o
      quadro que está soando e espia um à frente, já que a fila só deixa retirar. Cada amostra
      publicada carrega o índice do próprio quadro, então o teste confere, olhando só para o
      som, que o bloco mostrado é o carimbado. Com metade dos processadores queimando ciclos:
      0,08 ms contra 19,98 ms de um consumidor que seguisse o bloco mais novo.*

> **Marco atingido.** O erro de sincronismo é medido, não presumido: 0,04 ms na máquina quieta
> e 0,08 ms sob carga, contra os 16 ms de orçamento e os 20 ms de quem ignora o buffer do
> dispositivo. Construir a UI antes disto seria construir sobre um relógio errado.
>
> A medida separa "o modelo errou" de "o agendador atrapalhou": leitura demorada, entrega
> simulada atrasada ou vencida não contam, e a corrida falha se sobrar pouca amostra válida —
> uma máquina que não sustenta a medição precisa dizer isso, não devolver um número bonito.

### Etapa 4 — Camada de terminal
*Depende de: 0 (pode correr em paralelo com 2–3). Produz: superfície de desenho confiável.*

- [x] 4.1 `src/ui/term/`: modo raw, alternate screen, cursor oculto
      — *duas guardas, porque são dois estados: o modo raw é do dispositivo de terminal e não
      escreve nada; tela e cursor são bytes na saída, e por isso os testes os conferem num
      `Vec<u8>`, inclusive através de um pânico. O modo raw não tem teste automatizado — o CI
      não tem TTY; foi conferido com `stty -a` num pseudo-terminal (`script`). Sem consumidor
      até a etapa 5, como a primitiva da 3.1.*
- [ ] 4.2 **Restauração garantida** em saída normal, `SIGINT/SIGTERM/SIGHUP` e panic (RF-516)
- [ ] 4.3 Detecção de capacidade de cor 16/256/truecolor; `NO_COLOR`; `--mono` (RF-514)
- [ ] 4.4 Buffer de células e escrita com **coalescência de SGR**, um `write` por quadro
      (RF-515, RF-621)
- [ ] 4.5 Medição do throughput real do TTY e pacing adaptativo (RF-601, RF-622)
- [ ] 4.6 `SIGWINCH` e reflow responsivo (RF-513)
- [ ] 4.7 Harness de snapshot de terminal para os testes de UI

> **Marco:** um quadro cheio cabe no orçamento de bytes (RNF-07) e o terminal volta ao normal
> mesmo se o processo morrer de forma feia.

### Etapa 5 — Cromo do player
*Depende de: 3 e 4. Produz: o player utilizável. **Fecha a F1.***

- [ ] 5.1 Layout responsivo com prioridade de painéis (RF-513)
- [ ] 5.2 Marquee do título com rolagem contínua (RF-502)
- [ ] 5.3 Transporte com estado visual do botão ativo (RF-503)
- [ ] 5.4 Barra de seek ligada ao *fast-forward* silencioso (RF-504, RF-207)
- [ ] 5.5 Display de tempo alternável entre decorrido e restante (RF-505)
- [ ] 5.6 Sliders de volume e de balanço/separação estéreo (RF-506)
- [ ] 5.7 Barra de status com o equivalente tracker do rodapé do Winamp (RF-507)
- [ ] 5.8 Teclas vindas de `keymap.json` → ações; overlay de ajuda
- [ ] 5.9 Playlist: `.m3u`, shuffle, repeat, painel com faixa atual (RF-508, RF-702)
- [ ] 5.10 Modo **shade** de uma linha (RF-511)
- [ ] 5.11 Temas de paleta (RF-517) e `--no-ui` (RF-518)

> **Marco (F1):** player usável em 80×24, sincronismo < 16 ms, terminal sempre restaurado.

### Etapa 6 — Palco: visualizações determinísticas
*Depende de: 5. Produz: os três modos que não exigem motor generativo. **Fecha a F2.***

- [ ] 6.1 Abstração do palco: framebuffer `f32` RGB com **correção de proporção da célula**
      (RF-531) — sem isso, todo círculo vira elipse
- [ ] 6.2 Quantizadores: *pixel*, *rampa ASCII* e *híbrido* (RF-534)
- [ ] 6.3 Dithering ordenado para 16/256 cores (RF-535)
- [ ] 6.4 Espectro: FFT, escala logarítmica, bandas por oitava, picos em queda (RF-522)
- [ ] 6.5 Osciloscópio com **trigger em cruzamento de zero**; mono, estéreo e X/Y (RF-523)
- [ ] 6.6 Barras flutuantes com física de ataque/decaimento e peak-hold (RF-524, RF-603)
- [ ] 6.7 Alternância por tecla, ciclo automático e transição suave (RF-527)
- [ ] 6.8 Modo tela cheia com overlay que some (RF-512)
- [ ] 6.9 Degradação por orçamento, sem jamais arriscar o áudio (RF-528, RNF-04)
- [ ] 6.10 Snapshots por hash e benchmarks de tempo/quadro e bytes/quadro

> **Marco (F2):** RNF-01 (CPU) e RNF-07 (banda) **medidos** e dentro da meta.

### Etapa 7 — Motor generativo (MilkDrop em ASCII)
*Depende de: 6. Produz: o modo assinatura do projeto. **Fecha a F3.***

- [ ] 7.1 Buffer de realimentação com warp bilinear: zoom, rotação, deslocamento, ondulação
      (RF-532) — é o que produz os rastros e túneis
- [ ] 7.2 Camadas de desenho: onda deformada, partículas, campos procedurais, geometria (RF-533)
- [ ] 7.3 Pós-processamento: brilho, saturação, mapa de paleta
- [ ] 7.4 Schema e carregador de presets JSON, de `config/presets/` e do diretório do usuário
      (RF-536)
- [ ] 7.5 Sistema de ligações *parâmetro ← sinal musical*, com energia de áudio como fonte
      alternativa (RF-538, RF-539)
- [ ] 7.6 12 ou mais presets embutidos; auto-cycle com blend cruzado; `--viz-seed` (RF-537)
- [ ] 7.7 Orçamento de CPU declarado por preset, com abandono de qualidade antes do quadro
      (RF-540)
- [ ] 7.8 Avaliação visual de cada preset em 16 cores e em 80×24 — o que é bonito em truecolor
      costuma virar ruído em 16

> **Marco (F3):** um preset mal comportado degrada a imagem e **nunca** o áudio (RNF-04).

### Etapa 8 — Motor de reprodução próprio
*Depende de: 2 (para o oráculo) e 3 (para os barramentos). A etapa mais longa do projeto.*

- [ ] 8.1 Modelo `Song` completo e `Dialect` por tracker (RF-201)
- [ ] 8.2 Replayer por tick, **sample-exact**, fatiando o buffer nos limites de tick
      (RF-202, RF-203)
- [ ] 8.3 Mixer: interpolação selecionável, volume ramping, filtro DC e Amiga, separação
      estéreo (RF-301 a RF-306)
- [ ] 8.4 **Barramento por canal pré-mix** (RF-308) e **eventos musicais** (RF-210)
- [ ] 8.5 Loader MOD + quirks do ProTracker, cada um comentado com a fonte (RF-110)
- [ ] 8.6 Loader S3M + semântica ST3 (RF-120); canais OPL2 silenciados com aviso (RF-124)
- [ ] 8.7 Loader XM + quirks do FastTracker II (RF-130)
- [ ] 8.8 Loader IT: NNA/DCT/DCA, envelopes, **filtro ressonante**, IT214, amostras estéreo,
      canais virtuais com roubo de voz (RF-140, RF-204)
- [ ] 8.9 Duração exata por simulação seca (RF-206), seek por *fast-forward* (RF-207),
      detecção de fim e loop (RF-205)
- [ ] 8.10 Mute/solo no replayer preservando o estado dos efeitos (RF-208); overrides (RF-209)
- [ ] 8.11 Corpus de conformidade, regressão contra o oráculo e alvo de fuzzing **por loader**
- [ ] 8.12 `--engine` para alternar entre motor próprio e externo e comparar

> **Ordem obrigatória dos formatos: MOD → S3M → XM → IT.** A complexidade é estritamente
> crescente, e o IT reestrutura o mixer. Fazer IT antes é refazer tudo depois.

### Etapa 9 — Interface plena sobre o motor próprio
*Depende de: 7 e 8. **Fecha a F4.***

- [ ] 9.1 Osciloscópios e barras **por canal**, agora com PCM real (RF-308)
- [ ] 9.2 Ligações de preset a eventos de nota com precisão de amostra — RF-538 pleno
- [ ] 9.3 Mixer de canais: VU, nota, instrumento, mute/solo por canal (RF-509)
- [ ] 9.4 Painel de amostras/instrumentos e mensagem do módulo com scroller (RF-510)
- [ ] 9.5 Tracker view como modo opcional do palco, desligado por padrão (RF-526)
- [ ] 9.6 Cor determinística por instrumento e flash no disparo de nota (RF-605, RF-603)

> **Marco (F4):** o diferencial do projeto sai do papel — as visualizações reagem à música,
> não a uma estimativa dela.

### Etapa 10 — Periferia e integrações
*Depende de: 9. Cada item é independente e paralelizável.*

- [ ] 10.1 MPRIS no Linux, para teclas de mídia (RF-708)
- [ ] 10.2 Controle remoto por socket unix, `--ipc` (RF-708)
- [ ] 10.3 Mouse opcional na barra de seek e nos painéis, `--mouse` (RF-504)
- [ ] 10.4 Subsongs, `--duration`, `--start-order`, `--loop N` (RF-704, RF-205)
- [ ] 10.5 FLAC na exportação offline (RF-403)
- [ ] 10.6 Modo de baixa largura de banda revisado ponta a ponta (RF-518)
- [ ] 10.7 Decidir OPL2 do S3M: implementar ou manter fora com aviso explícito (RF-124)

### Etapa 11 — Endurecimento
*Depende de: 10. Sem esta etapa não existe 1.0, só um protótipo bonito.*

- [ ] 11.1 Fuzzing prolongado de todos os loaders, com corpus real e corrompido; zero achados
- [ ] 11.2 Medição formal de RNF-01 a RNF-07, com ajuste do que estiver fora da meta
- [ ] 11.3 Teste de resistência: 30 min contínuos sem xrun, sem vazamento, sem degradação
- [ ] 11.4 Teste em terminais reais: xterm, kitty, alacritty, wezterm, tmux, screen, Windows
      Terminal, e sobre SSH em link lento
- [ ] 11.5 Auditoria de licenças de todas as dependências (RNF-12)
- [ ] 11.6 **Varredura contra §4:** nenhum literal sem nome, nenhum valor sintonizável fora do
      JSON, nenhuma constante duplicada
- [ ] 11.7 Varredura contra §3: abstração sem uso real, wrapper que só encaminha, trait com uma
      implementação — remover

### Etapa 12 — Release 1.0
*Depende de: 11.*

- [ ] 12.1 Documentação de usuário: man page, `--help` completo, exemplos de `config.json`,
      guia de criação de presets
- [ ] 12.2 `CHANGELOG.md` e versionamento semântico
- [ ] 12.3 CI de release: build multiplataforma, artefatos e checksums
- [ ] 12.4 Empacotamento: `cargo publish`, Homebrew, AUR, `.deb`, Scoop (RNF-11)
- [ ] 12.5 Conferência final contra os **critérios de aceite** de
      [`docs/REQUISITOS.md` §11](docs/REQUISITOS.md#11-critérios-de-aceite)
- [ ] 12.6 Capturas e GIF do palco em ação no README

> **100 %:** todos os RF da §3 do documento de requisitos atendidos, RNF-01 a RNF-11 medidos e
> dentro da meta, corpus de conformidade passando, fuzzing sem achados e pacotes publicados
> para Linux, macOS e Windows.

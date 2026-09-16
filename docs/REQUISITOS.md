# xmcli — Levantamento de Requisitos

**Produto:** player de música tracker para terminal, com interface 100% em ASCII colorido e animado.
**Formatos alvo:** `.mod`, `.s3m`, `.xm`, `.it` (+ variantes e contêineres comprimidos).
**Status:** documento de requisitos (pré-implementação).

---

## 1. Visão e escopo

Um binário único, sem dependências de runtime, que toca módulos tracker no console com
qualidade de reprodução comparável à do OpenMPT.

**A interface não é um tracker — é um player.** O modelo mental é o do Winamp, não o do
FastTracker: janela compacta, transporte, título rolando, playlist, e um **palco de
visualização** que domina a tela e alterna entre modos (espectro, osciloscópio, barras
flutuantes, efeitos generativos estilo MilkDrop). A grade de padrões existe como um modo
secundário opcional, para quem quiser, e não como a tela principal.

**Dentro do escopo:** reprodução fiel, navegação, visualização, playlist, exportação para WAV.
**Fora do escopo (v1):** edição de módulos, MIDI-in, formatos exóticos (MED, PSM, FAR, AHX…),
plugins VST, servidor de streaming, importação de presets `.milk` originais.

---

## 2. Glossário operacional

| Termo | Definição usada neste documento |
|---|---|
| **Order** | posição na sequência de reprodução; aponta para um padrão |
| **Pattern** | matriz de linhas × canais com eventos (nota, instrumento, volume, efeito) |
| **Row** | uma linha do padrão |
| **Tick** | menor unidade temporal do replayer; efeitos são processados por tick |
| **Speed** | ticks por linha (efeito `Axx`/`Fxx`) |
| **Tempo (BPM)** | define a duração do tick: `amostras_por_tick = (taxa_amostragem * 2.5) / BPM` |
| **NNA** | New Note Action (IT): Cut / Continue / Note-off / Note-fade |
| **Canal virtual** | voz de mixagem alocada dinamicamente (IT chega a 256) |
| **Palco** | área principal da UI onde roda a visualização ativa |
| **Célula** | posição do terminal: um caractere ASCII + cor de frente + cor de fundo |

---

## 3. Requisitos funcionais

### 3.1 Carregadores de formato (RF-100)

Requisito transversal: **todo arquivo de entrada é dado não confiável**. Nenhum loader pode
confiar em tamanhos, offsets ou contagens declarados no cabeçalho; todos devem ser validados
contra o tamanho real do arquivo.

| ID | Requisito |
|---|---|
| RF-101 | Detecção de formato por conteúdo (magic/heurística), **não** por extensão |
| RF-102 | Recusar arquivos malformados com erro descritivo, sem panic/crash/leitura fora de limites |
| RF-103 | Carregar parcialmente módulos truncados quando o dano for recuperável (comum em coleções antigas) |
| RF-104 | Descompactar contêineres: `.zip`, `.gz`, `.xz`, `.bz2`, os empacotados `.mdz/.s3z/.xmz/.itz` e PowerPacker (`PP20`) |
| RF-105 | Suporte a leitura via `stdin` e via caminho |

**MOD (RF-110)** — ProTracker e derivados
- Variantes de assinatura: `M.K.`, `M!K!`, `4CHN`, `6CHN`, `8CHN`, `xxCH` (10–32), `FLT4/FLT8`,
  `CD81`, `OKTA`, `TDZ`, e o formato de 15 amostras sem assinatura (Ultimate Soundtracker).
- Amostras PCM 8-bit com sinal, loop com ponto/comprimento em words, finetune −8..+7.
- Pitch por **período Amiga** com tabela de períodos e clamp (113–856 no ProTracker).
- Efeitos `0`–`F` e subcomandos `E0`–`EF`.
- Timing CIA vs. VBlank (`Fxx` < 0x20 = speed, ≥ 0x20 = BPM); flag para modo VBlank puro.
- **Quirks obrigatórios** (módulos reais dependem deles): sample swap, comportamento do `9xx`
  (offset) além do fim da amostra, `E9x` retrigger, tabela de vibrato do PT, arpeggio wrap,
  `EEx` pattern delay combinado com `Dxx`, panning fixo LRRL da Paula.

**S3M (RF-120)** — Scream Tracker 3
- Até 32 canais; padrões em formato compactado; amostras 8/16-bit, com e sem sinal.
- Semântica ST3 de `Axx` (speed), `Txx` (tempo), `Vxx` (global volume), memória de portamento
  por efeito (diferente de XM/IT), `SBx` (pattern loop) com o bug de escopo por canal.
- Diferenças de mixagem GUS vs. SoundBlaster (volume de amostra, clamping) — configurável.
- Canais **Adlib/OPL2** (9 vozes FM melódicas): decisão de escopo — ver RF-124.
- RF-124: v1 emite aviso e silencia canais FM; v1.x integra emulação OPL2 (Nuked-OPL3/ymfm).

**XM (RF-130)** — FastTracker II
- Até 32 canais (espec.) / 64 na prática; 128 instrumentos × até 16 amostras, com keymap.
- Envelopes de volume e panning com pontos, sustain e loop; fadeout; vibrato de instrumento
  (tipo, sweep, depth, rate).
- Amostras 8/16-bit com **codificação delta**; ping-pong loop.
- Tabela de frequência **linear ou Amiga** (flag do módulo).
- Coluna de volume com efeitos próprios (vibrato, pan slide, portamento).
- **Quirks FT2 obrigatórios:** bug do `E6x` (pattern loop), ordem de avaliação do note delay
  (`EDx`), tratamento de envelope no tick 0, `Kxx` (key-off) com/sem instrumento, arredondamento
  do portamento linear. Módulos de demoparty soam errados sem eles.

**IT (RF-140)** — Impulse Tracker
- Até 64 canais de padrão e até 256 canais virtuais.
- **NNA** (Cut/Continue/Off/Fade) + **DCT/DCA** (duplicate check type/action).
- Envelopes de volume, panning e **pitch/filtro**, com sustain loop e carry.
- **Filtro passa-baixa ressonante** por canal (`Zxx` + macros MIDI `SFx`, tabela de macros do
  módulo); resposta compatível com o IT original.
- Compressão IT214 para amostras 8-bit e 16-bit; amostras estéreo.
- Modo "instrumento" vs. "amostra"; `Old Effects` e `Linear Slides` como flags do módulo.
- Cadeia de volume completa: sample vol → channel vol → instr. global vol → global vol → mix vol.
- Surround (`S91`), pan separation, sample vibrato.
- Extensões OpenMPT (MPTM/`IT` estendido): ler o que for possível, ignorar o resto sem falhar.

**RF-150 — Metadados**
- Título, mensagem/comentário, nomes de amostras e instrumentos, tracker de origem, número de
  canais/ordens/padrões, duração estimada, detecção de subsongs.
- **Transcodificação de texto:** nomes e mensagens são bytes em CP437 (trackers PC) ou
  ISO-8859-1/Amiga. Devem ser convertidos para UTF-8 preservando a arte ASCII embutida
  (extremamente comum em nomes de amostras). Nunca assumir UTF-8 na entrada.

### 3.2 Motor de reprodução (RF-200)

| ID | Requisito |
|---|---|
| RF-201 | Modelo de canção intermediário comum aos 4 formatos, com "dialeto" por formato preservado (não normalizar quirks para um denominador comum — é a causa nº 1 de reprodução errada) |
| RF-202 | Máquina de estados por tick: tick 0 processa nota/instrumento/efeito de disparo; ticks > 0 processam efeitos contínuos |
| RF-203 | Processamento **sample-exact**: o buffer de saída é fatiado nos limites de tick, nunca arredondado para o tamanho do bloco de áudio |
| RF-204 | Alocação de canais virtuais com política de roubo (menor volume/mais antigo) ao atingir o limite |
| RF-205 | Detecção de fim de música: loop explícito (`Bxx` para trás), contagem de repetições de order, modos `--loop N` / `--no-loop` |
| RF-206 | Cálculo de duração exata por simulação seca do replayer (sem mixagem), com cache |
| RF-207 | Seek para (order, row) arbitrário reconstruindo o estado por *fast-forward* silencioso |
| RF-208 | Mute/solo por canal aplicado no replayer, mantendo o estado dos efeitos |
| RF-209 | Override de speed/tempo/pitch global sem quebrar o estado do módulo |
| RF-210 | **Emissão de eventos musicais** para a UI: tick, nova row, novo order, mudança de BPM/speed e **disparo de nota por canal** (nota, instrumento, volume, pan). É a fonte de sincronia das visualizações — ver RF-521 |

### 3.3 Mixagem e DSP (RF-300)

| ID | Requisito |
|---|---|
| RF-301 | Interpolação selecionável: nearest (autêntica Amiga), linear, cúbica (Hermite), sinc/windowed |
| RF-302 | **Volume ramping** em transições de nota/volume para eliminar cliques (configurável, incl. desligado para timbre "cru") |
| RF-303 | Mixagem interna em `f32` com headroom; limiter/soft-clip opcional na saída |
| RF-304 | Filtro DC e filtro "Amiga LED" (~3.3 kHz) opcional para MOD |
| RF-305 | Stereo separation configurável (0 % mono … 100 % hard-pan Amiga), padrão ~35 % para MOD |
| RF-306 | Taxas de 22.05k–192 kHz; conversão de qualidade quando o dispositivo não aceitar a nativa |
| RF-307 | **Zero alocação e zero lock no callback de áudio** |
| RF-308 | Barramento paralelo de saída **por canal** (pré-mix) para osciloscópios e barras por canal. Reconstruir a onda a partir de "posição na amostra" na UI degrada com pitch alto e loops curtos |
| RF-309 | Barramento **master** (pós-mix, estéreo) alimentando osciloscópio, FFT e o detector de energia das visualizações |

### 3.4 Saída de áudio (RF-400)

| ID | Requisito |
|---|---|
| RF-401 | Backends: PipeWire/PulseAudio/ALSA (Linux), CoreAudio (macOS), WASAPI (Windows); automático com fallback e `--device` |
| RF-402 | Latência alvo configurável (padrão 20–40 ms), com detecção e recuperação de xruns |
| RF-403 | Exportação offline para WAV/FLAC mais rápida que tempo real (`--render out.wav`) |
| RF-404 | PCM cru para `stdout` (`--stdout`) para encadeamento com `ffmpeg`/`sox` |
| RF-405 | Backend nulo para benchmark e testes de regressão determinísticos |
| RF-406 | Relógio de reprodução baseado em **amostras efetivamente consumidas pelo dispositivo**, não em amostras mixadas — base de todo o sincronismo da UI (ver RF-620) |

### 3.5 Interface: player estilo Winamp (RF-500)

Regra de ouro: **ASCII de 7 bits (0x20–0x7E) é o modo padrão e sempre suficiente.** Unicode
(blocos, braille) é um modo opcional de realce, nunca requisito. A cor faz o trabalho pesado.

#### 3.5.1 Layout

```
+------------------------------------------------------------------------------+
| xmcli  >> SPACE DEBRIS -- by Captain -- greetings to ... <<   [8]  spectrum   |  <- marquee + modo
+------------------------------------------------------------------------------+
|                                                                              |
|                                                                              |
|                       P A L C O   D E   V I S U A L I Z A C A O              |  <- ocupa toda a
|                                                                              |     altura livre
|                                                                              |
+------------------------------------------------------------------------------+
| 01:23 / 04:57  |<  >  ||  []  >|      VOL [########----]  BAL [----|----]    |  <- transporte
| [=================================o------------------------------------]     |  <- barra de seek
| XM  8ch  44.1kHz  BPM 125  SPD 06  ORD 07/2A  ROW 3C   LOOP  INTERP:cubic    |  <- status
+------------------------------------------------------------------------------+
```

| ID | Requisito |
|---|---|
| RF-501 | **Palco de visualização** como elemento dominante: ocupa toda a altura não usada por cabeçalho, transporte e status |
| RF-502 | **Marquee do título**: rolagem horizontal contínua do título do módulo + nome do arquivo, com velocidade independente do tempo de reprodução; pausa ao final e reinicia (comportamento clássico) |
| RF-503 | **Transporte**: play/pause, stop, faixa anterior/próxima, com estado visual do botão ativo |
| RF-504 | **Barra de seek** operável por teclado (e por mouse, se `--mouse`), mostrando posição e duração; seek aplica RF-207 |
| RF-505 | **Display de tempo** alternável entre decorrido e restante |
| RF-506 | **Sliders** de volume e de balanço/stereo-separation |
| RF-507 | **Barra de status** com o equivalente tracker do "128 kbps / 44 kHz / stereo" do Winamp: formato, nº de canais, taxa de amostragem, BPM, speed, order/pattern/row, flags (loop, interpolação, filtro) |
| RF-508 | **Painel de playlist** sobreponível/lateral, com faixa atual destacada, duração por item, shuffle/repeat |
| RF-509 | **Painel "mixer de canais"** (o lugar do equalizador do Winamp, mas útil aqui): uma faixa por canal com VU, nota corrente, instrumento, e mute/solo — de 4 a 64 canais, com rolagem |
| RF-510 | **Painel de informações**: amostras/instrumentos e a mensagem/comentário do módulo, com scroller |
| RF-511 | **Modo "shade" (uma linha)**: player colapsado em uma única linha do terminal — título rolando, tempo, mini-VU. Ideal para deixar rodando no canto de um tmux |
| RF-512 | **Modo tela cheia de visualização**: palco ocupando 100 % do terminal, sem cromo, com overlay de informação que aparece na interação e some após alguns segundos |
| RF-513 | Layout responsivo com `SIGWINCH`: painéis se recolhem por prioridade; mínimo utilizável em 80×24; abaixo disso, degradar para o modo shade |
| RF-514 | Suporte a 16 / 256 / truecolor com **detecção de capacidade** (`TERM`, `COLORTERM`) e degradação automática; `--mono` e respeito a `NO_COLOR` |
| RF-515 | Renderização em buffer duplo com diff, alternate screen, modo raw, cursor oculto, um `write` por quadro |
| RF-516 | **Restauração garantida do terminal** em saída normal, `SIGINT/SIGTERM/SIGHUP` e panic |
| RF-517 | Temas de paleta (Winamp clássico, ProTracker, ST3, FT2, IT, monocromático âmbar/verde) |
| RF-518 | `--no-ui` (apenas log) e modo de baixa largura de banda para SSH/tmux |

**Teclas mínimas:** `space` play/pause · `z/x/c/v/b` transporte (padrão Winamp) · `←/→` seek ·
`↑/↓` volume · `Tab`/`F1..F6` painéis · `Enter` tela cheia · `w` modo shade · `1..9` modo de
visualização · `n` próximo preset · `r` aleatório/auto-cycle · `s` shuffle · `l` loop ·
`i` interpolação · `?` ajuda · `q` sair.

#### 3.5.2 Modos de visualização (RF-520)

| ID | Modo | Requisito |
|---|---|---|
| RF-521 | — | **Toda visualização é dirigida pelo barramento de visualização** (§4): PCM master, PCM por canal, bandas de FFT, e os **eventos musicais exatos** do replayer (RF-210) |
| RF-522 | **Espectro** | Analisador FFT com escala logarítmica de frequência, bandas por 1/1 ou 1/3 de oitava, suavização por ataque/decaimento assimétrico, marcadores de pico em queda, gradiente de cor por amplitude |
| RF-523 | **Osciloscópio** | Forma de onda do master com **trigger em cruzamento de zero** (sem isso a onda "treme"); modos mono, estéreo sobreposto, estéreo empilhado e X/Y (goniômetro); escala de tempo ajustável |
| RF-524 | **Barras flutuantes** | Barras por canal ou por banda, com física: ataque instantâneo, decaimento ~20 dB/s, pico flutuante com gravidade, cor por instrumento (determinística) e flash no disparo de nota |
| RF-525 | **Efeitos generativos ("MilkDrop ASCII")** | Motor de efeitos com feedback e presets — especificado em §3.6 |
| RF-526 | **Tracker view** *(opcional, off por padrão)* | Grade de padrões clássica como **um dos modos do palco**, para quem quiser; não é a tela principal |
| RF-527 | — | Alternância por tecla, por ciclo automático temporizado (`--viz-cycle 30s`) e por modo aleatório; **transição suave** (fade/blend) entre modos e presets |
| RF-528 | — | Cada modo declara seu custo de CPU e sua degradação: em terminal lento ou `--low-bandwidth`, reduzir resolução efetiva, fps e profundidade de cor antes de desabilitar |

### 3.6 Motor de efeitos generativos (RF-530)

Objetivo: o espírito do MilkDrop — campos animados, reativos à música, que evoluem e nunca
se repetem exatamente — em caracteres ASCII coloridos. **Não** é portar presets `.milk`
(a linguagem de equações do MilkDrop e o pipeline de shaders estão fora de escopo); é
reimplementar o *modelo*.

**Pipeline por quadro:**

```
  buffer float RGB (quadro anterior)
        |  1. decaimento + warp (zoom, rotação, deslocamento, ondulação)
        v     amostragem bilinear -> realimentação
  +-----------------+
  | campo de cor    |  2. desenho: waveform, formas, partículas, campos
  |   (f32 R,G,B)   |     dirigidos pelos parâmetros do preset e pela música
  +-----------------+
        |  3. pós: brilho, saturação, mapa de paleta
        v
  quantizador -> células do terminal (caractere + fg + bg)
```

| ID | Requisito |
|---|---|
| RF-531 | Framebuffer em ponto flutuante RGB na resolução de células, com **correção de proporção**: a célula do terminal é ≈ 1:2, então o espaço do efeito deve compensar (ou renderizar em 2× na horizontal e reduzir), senão todo círculo vira elipse |
| RF-532 | **Realimentação com warp**: o quadro anterior é reamostrado com zoom/rotação/deslocamento/ondulação e atenuado — é o que produz os rastros e túneis característicos |
| RF-533 | Camada de desenho: forma de onda deformada, partículas por canal, campos procedurais (plasma, ruído, interferência), formas geométricas |
| RF-534 | **Quantização selecionável** para célula: (a) *pixel* — caractere fixo + cor de fundo, imagem lisa; (b) *rampa ASCII* — luminância mapeada em rampa calibrada (`" .,:;irsXA253hMHGS#9B&@"`), cor no primeiro plano, estética clássica; (c) *híbrido* — fundo com a cor média e caractere/frente carregando o detalhe, melhor resolução aparente |
| RF-535 | Dithering opcional (ordenado) para suavizar bandeamento em 16/256 cores |
| RF-536 | **Presets declarativos** (JSON, validados por schema): parâmetros de warp, paleta, camadas de desenho e as ligações "parâmetro ← sinal musical". Presets são dados, carregáveis de `config/presets/` e de `$XDG_CONFIG_HOME/xmcli/presets/` |
| RF-537 | Conjunto embutido com no mínimo 12 presets distintos; auto-cycle com blend cruzado; semente aleatória por faixa para variação reprodutível (`--viz-seed`) |
| RF-538 | **Reatividade musical exata (diferencial do projeto):** o replayer entrega row, tick, BPM e disparos de nota por canal com precisão de amostra. Um efeito pode acertar a batida **sem detector de onset heurístico** — algo que nenhum visualizador de áudio genérico consegue. Ligações disponíveis: energia por banda, energia total, batida por row/beat, disparo de nota em canal *N*, nota/oitava tocada, instrumento ativo, order/pattern corrente |
| RF-539 | Detecção de energia/onset em áudio ainda disponível como fonte alternativa, para robustez e para o modo de entrada externa |
| RF-540 | Orçamento de CPU declarado por preset; o motor abandona qualidade (resolução, camadas) antes de perder o quadro |

### 3.7 Animação, sincronismo e orçamento de banda (RF-600)

| ID | Requisito |
|---|---|
| RF-601 | Taxa de quadros alvo 30–60 fps, desacoplada do áudio, com pacing adaptativo |
| RF-602 | Toda animação deriva do **tempo de reprodução audível**, nunca do relógio de parede |
| RF-620 | **Compensação de latência:** posição exibida = posição mixada − amostras ainda no buffer do dispositivo. Erro alvo **< 1 quadro (≈ 16 ms)**. Sem isso a visualização "adianta" o som — o defeito mais perceptível deste tipo de programa |
| RF-603 | Animações com decaimento físico (ataque rápido, queda lenta, peak-hold); nada de valores saltando de quadro em quadro |
| RF-604 | Sem *tearing* e sem piscar: um quadro completo por `write(2)` |
| RF-605 | Cores por instrumento/canal determinísticas (mesmo instrumento → mesma cor entre execuções) |
| RF-621 | **Orçamento de bytes por quadro.** Um palco de visualização repinta quase todas as células a cada quadro — o render por diff ajuda pouco aqui. Em truecolor, uma célula pode custar ~20 bytes de SGR: 80×24 ≈ 40 KB/quadro ≈ 2,4 MB/s a 60 fps, o que estrangula SSH e tmux. Obrigatório: **coalescência de SGR** (emitir a sequência de cor só quando muda em relação à célula anterior), orçamento configurável (`--max-frame-bytes`) e degradação em cascata: truecolor → 256 → 16 cores, modo pixel → rampa ASCII, redução de fps, redução de resolução do palco |
| RF-622 | Medir o throughput real do TTY (tempo do `write` bloqueante) e adaptar em runtime, sem intervenção do usuário |

### 3.8 CLI, playlist e configuração (RF-700)

| ID | Requisito |
|---|---|
| RF-701 | `xmcli [opções] <arquivos|diretórios|playlists...>`; diretórios varridos recursivamente com filtro por formato |
| RF-702 | Playlists `.m3u`/`.m3u8`; shuffle, repeat-one/all, próxima/anterior |
| RF-703 | `--info` imprime metadados em texto e em **JSON** (`--json`) para scripts |
| RF-704 | `--render`, `--stdout`, `--duration`, `--start-order`, `--subsong`, `--loop N` |
| RF-705 | `--viz <modo>`, `--viz-cycle`, `--preset`, `--viz-seed`, `--fullscreen`, `--shade`, `--no-fx` |
| RF-706 | **Configuração compartilhada em JSON como fonte única da verdade** (`config/defaults.json`, `keymap.json`, `palettes.json`, validados por schema), sobreposta por `$XDG_CONFIG_HOME/xmcli/config.json`; precedência CLI > env > usuário > padrão. Resolvida uma vez na inicialização para uma estrutura congelada — nenhum laço quente lê configuração |
| RF-707 | Códigos de saída estáveis (0 ok, 1 erro de execução, 2 uso inválido, 3 formato não suportado) |
| RF-708 | Integração opcional MPRIS (Linux) para teclas de mídia; controle remoto por socket unix (`--ipc`) |
| RF-709 | Logs em `stderr`, nunca poluindo a TUI; `-v/-vv` |

---

## 4. Requisitos não funcionais

| ID | Requisito | Métrica de aceite |
|---|---|---|
| RNF-01 | CPU | módulo IT de 64 canais + palco generativo a 60 fps em 120×40 < **12 %** de um núcleo moderno (áudio isolado < 3 %) |
| RNF-02 | Latência de áudio | ≤ 40 ms padrão; ≤ 10 ms configurável sem xruns em máquina ociosa |
| RNF-03 | Determinismo do áudio | callback sem alocação, sem mutex, sem I/O, sem `panic` |
| RNF-04 | Isolamento de falhas | um preset de visualização que estoure o orçamento **nunca** pode causar xrun: a UI é sempre a que perde quadros |
| RNF-05 | Memória | < 64 MB para um módulo de 32 MB de amostras |
| RNF-06 | Startup | primeiro som em < 150 ms para módulo típico |
| RNF-07 | Banda do terminal | ≤ 400 KB/s por padrão em modo cheio; ≤ 60 KB/s em `--low-bandwidth` |
| RNF-08 | Portabilidade | Linux (x86_64, aarch64), macOS, Windows, *BSD; binário estático quando possível |
| RNF-09 | Segurança | parsers sem UB; fuzzing contínuo em CI; nenhum crash em corpus malformado |
| RNF-10 | Robustez do terminal | terminal sempre restaurado, inclusive em panic e em `kill -TERM` |
| RNF-11 | Empacotamento | cargo-install, Homebrew, AUR, `.deb`, Scoop, binários por release |
| RNF-12 | Licenciamento | atenção ao motor de terceiros: libopenmpt (BSD-3) vs. libxmp — auditar antes de vincular |

---

## 5. Arquitetura

Três domínios de tempo, sem bloqueio entre eles. **O barramento de visualização é a espinha
dorsal desta arquitetura** — ele é o que torna o palco possível.

```
 [thread de UI  30-60 Hz]              [thread de audio - tempo real]      [thread de I/O]
  +--------------------------+          +--------------------------+      +--------------+
  | teclado / resize         |          | replayer (por tick)      |      | carregar     |
  | motor de visualizacao    |<== BUS ==| mixer + DSP              |<-----| modulo       |
  |   framebuffer f32        |  (SPSC)  | barramento por canal     |      | playlist     |
  |   quantizador -> celulas |          | barramento master        |      | presets      |
  | cromo (transporte/status)|          | eventos musicais         |      +--------------+
  | coalescencia SGR + write |          +--------------------------+
  +--------------------------+                     |
            |  comandos (SPSC)                     v
            +------------------------------> dispositivo de audio
```

**Barramento de visualização (SPSC, sem lock, escrito pelo áudio, lido pela UI):**

| Canal do barramento | Conteúdo | Consumidores |
|---|---|---|
| PCM master | ring de N ms, estéreo, pós-mix | osciloscópio, FFT, energia |
| PCM por canal | ring de N ms, pré-mix, por canal | scopes por canal, partículas |
| Snapshot de estado | order/row/tick/BPM/speed; por canal: nota, instr., vol, pan, pico | cromo, mixer de canais, barras |
| Eventos musicais | disparo de nota, nova row, novo order, mudança de tempo | motor de efeitos (RF-538) |

Cada item carrega o **carimbo de amostra** em que ocorre. A UI consome o que corresponde ao
tempo *audível* (RF-620), não o mais recente produzido — é isso que mantém a imagem colada no som.

**Comandos** (mute, seek, volume, troca de preset) vão da UI ao áudio por fila SPSC e são
aplicados em limites de tick.

---

## 6. Decisão de motor: escrever vs. reusar

| Opção | Prós | Contras |
|---|---|---|
| **Motor próprio** | controle total do barramento de visualização (RF-308/309/210), sem FFI | caro em precisão: os quirks de PT/ST3/FT2/IT são anos de engenharia reversa |
| **libopenmpt (FFI)** | precisão de referência, mantida ativamente, cobre os 4 formatos | não expõe PCM por canal nem eventos de nota — **limita exatamente o que a nova UI mais precisa** |
| **libxmp (FFI)** | leve, C puro, `xmp_frame_info` expõe estado por canal | precisão inferior à do OpenMPT em casos de borda de IT/XM |

**Recomendação (revista pela nova UI):** a mudança de interface aumenta o valor do motor
próprio, porque as visualizações vivem de dados que as bibliotecas prontas não expõem.
Mantém-se a estratégia em duas fases, com a fase 3 promovida em prioridade:

1. **MVP** com FFI para **libopenmpt**: som correto desde o primeiro dia. Visualizações que
   dependem só de PCM master (espectro, osciloscópio, efeitos generativos por energia)
   funcionam plenamente; as por canal ficam aproximadas por VU + estado de canal.
2. **v1.x — motor próprio em Rust**, validado contra o libopenmpt como oráculo de regressão,
   habilitando barramento por canal e eventos de nota exatos. É o que destrava RF-538 na
   forma plena. Manter o backend libopenmpt como opção de comparação (`--engine`).

---

## 7. Escolha da linguagem

Restrições que decidem: (a) callback de áudio em tempo real → **sem GC**; (b) parsing de
arquivos binários hostis → **segurança de memória**; (c) **60 fps de aritmética de ponto
flutuante sobre um framebuffer** para o palco generativo; (d) binário único multiplataforma;
(e) FFI barato com libopenmpt/libxmp.

O item (c) é novo e reforça a mesma conclusão: o motor de efeitos é um laço numérico
apertado, sensível a layout de memória e a auto-vetorização.

| Linguagem | Áudio RT | Segurança | Palco 60 fps | TUI | Distribuição | Veredito |
|---|---|---|---|---|---|---|
| **Rust** | ótimo | ótimo | ótimo (SIMD, sem GC) | `ratatui`+`crossterm` | binário estático | **Recomendada** |
| C++17/20 | ótimo | fraco | ótimo | FTXUI/ncurses | build trabalhosa | Vice-líder |
| C | ótimo | pior | ótimo | ncurses | boa | portabilidade extrema apenas |
| Zig | ótimo | melhor que C | ótimo | imaturo | ótima | promissor, ecossistema insuficiente |
| Go | ruim (GC/cgo) | boa | mediano (GC no laço) | tcell/bubbletea | ótima | descartada |
| C#/Java | ruim (GC) | boa | mediano | razoável | runtime pesado | descartada |
| Python | inviável (GIL) | boa | inviável | textual/rich | ruim | só protótipo |

**Veredito: Rust.** Única opção que atende simultaneamente ao tempo real do áudio (RNF-02/03),
à segurança nos parsers (RNF-09) e ao orçamento numérico do palco (RNF-01), com TUI madura e
binário único. C++20 + libopenmpt + FTXUI é o vice-líder legítimo e a escolha certa **apenas**
se a equipe já dominar C++.

### Stack de referência (Rust)

| Camada | Crate |
|---|---|
| Áudio | `cpal` (backends nativos) — alternativa `sdl2` |
| TUI | `ratatui` + `crossterm` (com camada própria de coalescência SGR para o palco, RF-621) |
| CLI / config | `clap` (derive), `serde` + `serde_json`, `jsonschema`, `directories` |
| Erros / log | `thiserror`, `anyhow`, `tracing` |
| DSP | `realfft`/`rustfft` (espectro), `rubato` (resample de saída) |
| Visualização | código próprio; `glam` para vetor/matriz, `noise` para campos procedurais, `palette` para conversão de espaço de cor |
| IPC lock-free | `ringbuf`/`crossbeam` (SPSC), `triple_buffer` (snapshots) |
| Texto | `codepage-437` / `encoding_rs` |
| Contêineres | `zip`, `flate2`, `xz2` |
| Motor (fase 1) | FFI para `libopenmpt` (verificar o estado dos crates disponíveis antes de fixar) |
| Testes | `criterion`, `cargo-fuzz`, `insta` (snapshots de UI) |

---

## 8. Plano de fases

| Fase | Entrega | Critério de saída |
|---|---|---|
| **F0 — Esqueleto** | CLI, carregamento, saída de áudio, `--info` | toca um `.xm` sem UI, sem xruns |
| **F1 — Player** | cromo Winamp: marquee, transporte, seek, status, playlist, modo shade | usável em 80×24, sincronismo < 1 quadro |
| **F2 — Palco** | espectro, osciloscópio, barras flutuantes; coalescência SGR e orçamento de banda | RNF-01 e RNF-07 medidos e dentro da meta |
| **F3 — Generativo** | motor de efeitos com feedback, quantizadores, 12+ presets, auto-cycle | preset ruim degrada a UI, nunca o áudio (RNF-04) |
| **F4 — Motor próprio** | replayer Rust MOD → S3M → XM → IT + barramento por canal e eventos de nota | regressão contra oráculo dentro da tolerância; RF-538 pleno |
| **F5 — Polimento** | mixer de canais, tracker view opcional, MPRIS/IPC, exportação, empacotamento | pacotes publicados, fuzzing limpo em CI |

Ordem dos formatos no motor próprio: **MOD → S3M → XM → IT** (o IT reestrutura o mixer com
NNA, filtro ressonante e canais virtuais).

---

## 9. Estratégia de testes

1. **Corpus de conformidade:** o conjunto de módulos de teste do OpenMPT cobre exatamente os
   quirks por formato; cada caso vira teste automatizado.
2. **Regressão de render:** renderizar offline com semente fixa e comparar com referência
   (bit-a-bit no motor próprio; por RMS/correlação entre motores diferentes).
3. **Fuzzing:** `cargo-fuzz` por loader, corpus semeado com módulos reais e arquivos
   truncados/corrompidos; CI falha em qualquer crash ou timeout.
4. **Testes de visualização:** o motor de efeitos é determinístico dada (semente, entrada
   musical) → snapshots do framebuffer quantizado por preset, comparados por hash.
5. **Testes de UI:** snapshots do buffer do terminal em 80×24 / 120×40 / 200×60 e em
   16/256/truecolor.
6. **Orçamento:** benchmark de bytes/quadro e de tempo/quadro por preset, com regressão de
   desempenho no CI.
7. **Escuta comparativa:** A/B contra OpenMPT/libxmp em módulos conhecidos por explorar
   bugs de tracker.

---

## 10. Riscos

| Risco | Impacto | Mitigação |
|---|---|---|
| Precisão de reprodução (quirks) | alto | fase 1 com libopenmpt; motor próprio validado por oráculo |
| Sincronismo A/V incorreto | alto (é o que o usuário percebe) | RF-620 desde a F1, com medição automatizada |
| **Banda do terminal no palco** | **alto** | RF-621/622: coalescência SGR, orçamento de bytes, degradação em cascata |
| Visualização competindo com o áudio por CPU | alto | RNF-04: prioridade de thread, orçamento por preset, quadro descartado antes de qualquer risco de xrun |
| Efeitos generativos "bonitos no vídeo, ilegíveis no terminal" | médio | correção de proporção (RF-531), três quantizadores (RF-534), avaliação visual por preset em 16 cores |
| Dados por canal indisponíveis na fase 1 | médio | modos que dependem deles marcados como "aproximados" até a F4 |
| Terminal corrompido após crash | médio | hook de panic + handlers de sinal restaurando o TTY |
| Emulação OPL2 do S3M | baixo | fora do escopo da v1, com aviso explícito |
| Licenças de motor de terceiros | médio | auditar libopenmpt/libxmp antes de vincular |

---

## 11. Critérios de aceite

**MVP (F2):** toca corretamente os 4 formatos; cromo Winamp completo (marquee, transporte,
seek, status, playlist, shade); palco com espectro, osciloscópio e barras flutuantes; erro de
sincronismo < 16 ms; ASCII 7-bit em 16 cores; 80×24; ≤ 400 KB/s de saída no terminal; sem
xruns em 30 min contínuos; terminal restaurado em todas as formas de saída.

**1.0:** todos os RF de §3; RNF-01..11 medidos e dentro da meta; motor de efeitos com 12+
presets e auto-cycle; corpus de conformidade passando; fuzzing sem achados; pacotes publicados
para Linux/macOS/Windows.

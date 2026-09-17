//! Erro de sincronismo entre o que soa e o que a interface enxerga (RF-620).
//!
//! O alvo é menos de um quadro de vídeo, ≈ 16 ms. São duas corridas:
//!
//! 1. o **relógio** sozinho, na máquina quieta — a posição que ele informa contra a posição
//!    verdadeira (etapa 3.4);
//! 2. o **consumidor** do barramento master, com a máquina sob carga — o som que a interface
//!    escolheria mostrar contra o som que está soando (etapa 3.5).
//!
//! Um dispositivo de verdade não serve de medida aqui: o contêiner de integração contínua não
//! tem placa de som, e o erro que se quer medir não é o do hardware, é o do modelo. Então as
//! duas corridas simulam um dispositivo que consome em ritmo constante.
//!
//! Nada disto roda em tempo real de verdade: o sistema pode tirar qualquer uma das linhas de
//! execução do processador por dezenas de milissegundos. Uma pausa dessas atrasa a entrega
//! simulada ou a própria leitura, e o que ela mede é o agendador, não o relógio. Por isso cada
//! amostra é validada antes de contar (ver [`read`]), e o teste exige que uma fração delas
//! valha — em uma máquina onde quase nada vale, a medida não significa nada e o teste falha
//! dizendo isso. A medição contra hardware real fica para a etapa 11.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use xmcli::audio::clock::Clock;
use xmcli::bus::master::{self, Block};
use xmcli::bus::{Receiver, Stamped};
use xmcli::player::CHANNELS;

const RATE: u32 = 44_100;

/// Quadros por entrega do dispositivo simulado.
///
/// 882 quadros a 44,1 kHz são 20 ms: mais que o alvo de 16 ms de propósito, para que a medida
/// sem compensação estoure o orçamento e a medida com compensação tenha de caber nele.
const BUFFER_FRAMES: u64 = 882;

/// Entregas de uma corrida — 40 × 20 ms, pouco menos de um segundo de medição.
const DELIVERIES: u64 = 40;

/// O orçamento de RF-620: um quadro de vídeo a 60 fps.
const BUDGET_MS: f64 = 16.0;

/// Período entre leituras, bem mais apertado que os 16,7 ms de um quadro de vídeo.
const SAMPLE_PERIOD: Duration = Duration::from_millis(1);

/// Folga que separa "o modelo errou" de "o sistema operacional atrapalhou".
///
/// Uma leitura que demore mais que isto, ou uma entrega que atrase mais que isto, mediria a
/// pausa do agendador. É pequena o bastante para não esconder erro de modelo: o orçamento que
/// se quer verificar é dezesseis vezes maior.
const TOLERANCE: Duration = Duration::from_millis(1);

/// As duas corridas não podem se sobrepor.
///
/// Ambas medem tempo, e a segunda ocupa todos os processadores de propósito. Rodando junto,
/// cada uma mediria a outra. O harness de teste roda em paralelo por padrão, então a exclusão
/// precisa ser explícita.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

/// Duração de um buffer, que é o período entre entregas do dispositivo simulado.
fn buffer_period() -> Duration {
    Duration::from_secs_f64(BUFFER_FRAMES as f64 / f64::from(RATE))
}

/// Horário da entrega de índice `delivery`, contado do início da corrida.
///
/// O horário é absoluto, e não acumulado a partir da entrega anterior: um dispositivo de
/// verdade não atrasa a entrega seguinte porque a anterior atrasou, e somar períodos faria o
/// atraso de cada pausa do agendador ficar no relógio para sempre.
fn delivery_instant(start: Instant, delivery: u64) -> Instant {
    start + buffer_period().mul_f64(delivery as f64)
}

/// Espera até `deadline` cedendo a vez, sem dormir.
///
/// `sleep` erra por milissegundos em alguns sistemas — justamente a grandeza que este teste
/// mede.
fn spin_until(deadline: Instant) {
    while Instant::now() < deadline {
        std::thread::yield_now();
    }
}

/// Quadros que a régua verdadeira acumula em `elapsed`.
fn frames_in(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * f64::from(RATE)
}

/// Erro em milissegundos entre uma posição informada e a verdadeira.
fn error_ms(reported: u64, truth: f64) -> f64 {
    (reported as f64 - truth).abs() / f64::from(RATE) * 1_000.0
}

/// Dispositivo simulado: entrega um buffer por período, no horário.
struct Device {
    thread: JoinHandle<()>,
    /// Início da corrida, medido dentro da linha do dispositivo.
    ///
    /// A régua verdadeira nasce na primeira entrega, e não na chamada a `spawn`: o atraso da
    /// criação da linha de execução mediria o agendador do sistema, não o modelo de latência.
    start: Instant,
    /// Atraso da entrega mais recente, em nanossegundos.
    ///
    /// Começa em [`u64::MAX`] para que nenhuma leitura anterior à primeira entrega conte.
    late_nanos: Arc<AtomicU64>,
}

/// Põe o dispositivo simulado para rodar, chamando `render` antes de cada entrega.
///
/// `render` recebe o quadro em que o buffer começa e é onde o lado do áudio publica o que
/// acabou de produzir — na ordem do caminho real, o som está no anel antes de o dispositivo o
/// tornar audível.
fn spawn_device(
    clock: Arc<Clock>,
    running: Arc<AtomicBool>,
    mut render: impl FnMut(u64) + Send + 'static,
) -> Device {
    let late_nanos = Arc::new(AtomicU64::new(u64::MAX));
    let device_late = Arc::clone(&late_nanos);
    let (started, start) = std::sync::mpsc::channel();

    let thread = std::thread::spawn(move || {
        let start = Instant::now();
        started.send(start).expect("o teste espera pelo início");

        for delivery in 0..DELIVERIES {
            render(delivery * BUFFER_FRAMES);

            let due = delivery_instant(start, delivery);
            spin_until(due);

            // A entrega inteira fica marcada como inválida enquanto acontece, e só depois o
            // atraso real é publicado. O atraso é medido do horário devido até **depois** de a
            // âncora ser escrita, porque o agendador pode tirar esta linha do processador entre
            // uma coisa e outra — e nesse caso é a âncora que sai atrasada, não o laço de
            // espera.
            device_late.store(u64::MAX, Ordering::Release);
            clock.deliver(BUFFER_FRAMES);
            let late = Instant::now().duration_since(due);
            device_late.store(late.as_nanos() as u64, Ordering::Release);
        }

        running.store(false, Ordering::Release);
    });

    Device {
        thread,
        start: start.recv().expect("a linha do dispositivo começou"),
        late_nanos,
    }
}

/// Uma leitura confiável do relógio, com a verdade correspondente.
struct Reading {
    audible: u64,
    played: u64,
    truth: f64,
}

/// Lê a posição e a verdade do instante, ou devolve `None` se a leitura não for confiável.
///
/// Confiável é: a leitura coube na tolerância, e a última entrega do dispositivo simulado saiu
/// no horário. O segundo filtro não esconde defeito do modelo — cada entrega recorrige a
/// âncora do relógio, então uma entrega atrasada desloca a âncora pelo tamanho do atraso, e o
/// que se mediria até a entrega seguinte seria a pausa do agendador. Um dispositivo de verdade
/// não é preemptado; um dispositivo simulado que ficou sem processador simplesmente não é um
/// dispositivo.
///
/// São dois filtros, porque são dois jeitos de o dispositivo simulado falhar: a entrega que
/// **aconteceu** atrasada, que desloca a âncora, e a entrega **vencida que ainda não
/// aconteceu**, que deixa o relógio parado no teto do que já foi entregue. O atraso publicado
/// só conta a primeira; a segunda aparece como quadros entregues abaixo do que o horário pedia.
///
/// O atraso é conferido antes e depois das leituras, e precisa ser o mesmo: um valor diferente
/// significa que uma entrega aconteceu no meio, e que a âncora usada pode não ser a que o
/// atraso descreve.
fn read(clock: &Clock, device: &Device) -> Option<Reading> {
    let late = device.late_nanos.load(Ordering::Acquire);
    if late > TOLERANCE.as_nanos() as u64 {
        return None;
    }

    let before = Instant::now();
    let audible = clock.audible_frame();
    let played = clock.frames_played();
    let after = Instant::now();

    let window = after.duration_since(before);
    if window > TOLERANCE || device.late_nanos.load(Ordering::Acquire) != late {
        return None;
    }

    // O ponto médio da janela é o instante a que as duas leituras se referem.
    let elapsed = before.duration_since(device.start) + window / 2;
    let due = elapsed.as_secs_f64() / buffer_period().as_secs_f64();
    if played < (due as u64 + 1) * BUFFER_FRAMES {
        return None;
    }

    Some(Reading {
        audible,
        played,
        truth: frames_in(elapsed),
    })
}

#[test]
fn a_compensacao_de_latencia_cabe_no_orcamento_de_um_quadro() {
    /// Fração mínima de amostras válidas para a corrida valer como medição.
    const MIN_VALID: f64 = 0.8;

    let _exclusive = EXCLUSIVE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    let clock = Clock::new(RATE);
    let running = Arc::new(AtomicBool::new(true));
    let device = spawn_device(Arc::clone(&clock), Arc::clone(&running), |_| {});

    // O lado da interface: pergunta a posição como faria a cada quadro, só que mais vezes.
    let mut worst_compensated = 0.0_f64;
    let mut worst_naive = 0.0_f64;
    let mut valid = 0_u32;
    let mut total = 0_u32;
    let mut next = device.start;
    while running.load(Ordering::Acquire) {
        spin_until(next);
        next += SAMPLE_PERIOD;
        total += 1;

        let Some(reading) = read(&clock, &device) else {
            continue;
        };
        worst_compensated = worst_compensated.max(error_ms(reading.audible, reading.truth));
        worst_naive = worst_naive.max(error_ms(reading.played, reading.truth));
        valid += 1;
    }

    device
        .thread
        .join()
        .expect("a linha do dispositivo não entra em pânico");

    // Visível com `--nocapture`: a margem contra o orçamento é o que diz se a medida está
    // perto de falhar em uma máquina mais lenta.
    println!(
        "{valid} de {total} amostras válidas; erro máximo {worst_compensated:.2} ms com \
         compensação, {worst_naive:.2} ms sem"
    );

    assert!(
        f64::from(valid) >= f64::from(total) * MIN_VALID,
        "só {valid} de {total} leituras foram confiáveis: a máquina não sustentou a medição"
    );
    assert!(
        worst_compensated < BUDGET_MS,
        "erro de sincronismo de {worst_compensated:.2} ms, acima do orçamento de RF-620"
    );
    assert!(
        worst_naive > worst_compensated,
        "sem compensação o erro foi de {worst_naive:.2} ms, não maior que os \
         {worst_compensated:.2} ms compensados — a medida não está medindo nada"
    );
}

/// Ciclos que cada linha de carga queima entre duas consultas ao sinalizador de parada.
///
/// Grande o bastante para a consulta não dominar o laço, pequeno o bastante para as linhas
/// pararem assim que a corrida termina.
const LOAD_CHUNK: u64 = 4_096;

/// Põe o processador inteiro para trabalhar até `running` cair.
///
/// Metade dos processadores, que é a situação que RF-620 tem de suportar: a interface
/// disputando CPU com o resto do sistema, preemptada o tempo todo.
///
/// Metade, e não todos, porque duas das linhas que sobram são o dispositivo simulado e o
/// medidor. Afogando o dispositivo o teste passaria a medir o agendador do sistema — e um
/// dispositivo de verdade não é preemptado. A carga precisa atrapalhar o consumidor, não
/// substituir o objeto da medida.
fn spawn_load(running: &Arc<AtomicBool>) -> Vec<JoinHandle<()>> {
    let workers = std::thread::available_parallelism().map_or(1, |count| count.get() / 2);
    let workers = workers.max(1);
    (0..workers)
        .map(|_| {
            let running = Arc::clone(running);
            std::thread::spawn(move || {
                let mut burned = 0_u64;
                while running.load(Ordering::Relaxed) {
                    for cycle in 0..LOAD_CHUNK {
                        burned = burned.wrapping_add(cycle);
                    }
                    std::hint::black_box(burned);
                }
            })
        })
        .collect()
}

/// O lado da interface: segue o barramento master no tempo audível.
///
/// Guarda o bloco que contém o quadro que está soando e descarta o que já passou. O bloco que
/// ainda não chegou a hora de mostrar fica retido em [`Viewer::ahead`]: a fila só deixa
/// retirar, não devolver, e o consumidor precisa espiar um bloco à frente para saber que parou
/// no lugar certo.
struct Viewer {
    receiver: Receiver<Block>,
    current: Option<Stamped<Block>>,
    ahead: Option<Stamped<Block>>,
}

impl Viewer {
    /// Avança até o bloco que cobre `audible` e devolve o quadro que ele consegue mostrar.
    ///
    /// Quando o barramento alcançou o tempo audível, o quadro devolvido é o próprio `audible`.
    /// Quando ficou para trás — ou correu à frente — é o quadro mais próximo que existe no
    /// bloco retido, e a distância entre um e outro é o erro que RF-620 limita.
    fn frame_at(&mut self, audible: u64) -> Option<u64> {
        while let Some(next) = self.ahead.take().or_else(|| self.receiver.recv()) {
            if next.at_frame > audible {
                self.ahead = Some(next);
                break;
            }
            self.current = Some(next);
        }

        let current = self.current.as_ref()?;
        let last = current.at_frame + current.payload.frames() as u64 - 1;
        Some(audible.clamp(current.at_frame, last))
    }

    /// Confere que o som guardado no bloco é mesmo o do quadro carimbado.
    ///
    /// O produtor grava em cada amostra o índice do seu próprio quadro, então um carimbo que
    /// mentisse — ou um bloco fora de ordem — apareceria aqui como valor trocado.
    fn assert_frame_matches_audio(&self, frame: u64) {
        let current = self.current.as_ref().expect("há bloco para conferir");
        let offset = (frame - current.at_frame) as usize * CHANNELS;
        assert_eq!(
            current.payload.samples()[offset],
            frame as f32,
            "o bloco carimbado em {} não carrega o som do quadro {frame}",
            current.at_frame
        );
    }
}

#[test]
fn o_barramento_master_segue_o_tempo_audivel_sob_carga() {
    /// Histórico do barramento master nesta corrida — o padrão de `bus.master_history_ms`.
    ///
    /// Meio segundo cobre qualquer pausa do agendador aqui, e por isso o teste pode exigir que
    /// **nenhum** bloco se perca: uma perda seria uma configuração apertada demais, não um
    /// consumidor lento.
    const HISTORY_MS: u16 = 500;

    /// Fração mínima de amostras válidas com a máquina sob carga.
    ///
    /// Mais baixa que a da corrida quieta porque as linhas de carga existem justamente para
    /// tirar as outras do processador: o dispositivo simulado vai atrasar, e essas amostras não
    /// contam.
    const MIN_VALID: f64 = 0.4;

    let _exclusive = EXCLUSIVE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    let clock = Clock::new(RATE);
    let running = Arc::new(AtomicBool::new(true));
    let load = spawn_load(&running);

    let (mut sender, receiver) = master::channel(RATE, HISTORY_MS);
    // Cada amostra vale o índice do seu próprio quadro: é assim que o consumidor consegue
    // dizer, olhando só para o som, que quadro ele está mostrando.
    let mut buffer = vec![0.0_f32; BUFFER_FRAMES as usize * CHANNELS];
    let device = spawn_device(
        Arc::clone(&clock),
        Arc::clone(&running),
        move |first_frame| {
            for (index, sample) in buffer.iter_mut().enumerate() {
                *sample = (first_frame + (index / CHANNELS) as u64) as f32;
            }
            master::publish(&mut sender, first_frame, &buffer);
        },
    );

    let mut viewer = Viewer {
        receiver,
        current: None,
        ahead: None,
    };
    let mut worst_shown = 0.0_f64;
    let mut worst_newest = 0.0_f64;
    let mut valid = 0_u32;
    let mut total = 0_u32;
    let mut next = device.start;
    while running.load(Ordering::Acquire) {
        spin_until(next);
        next += SAMPLE_PERIOD;
        total += 1;

        let Some(reading) = read(&clock, &device) else {
            continue;
        };
        let Some(shown) = viewer.frame_at(reading.audible) else {
            continue;
        };
        viewer.assert_frame_matches_audio(shown);

        worst_shown = worst_shown.max(error_ms(shown, reading.truth));
        // O que um consumidor sem compensação mostraria: o bloco mais novo que chegou, cujo
        // último quadro é o que acabou de ser entregue.
        worst_newest = worst_newest.max(error_ms(reading.played - 1, reading.truth));
        valid += 1;
    }

    device
        .thread
        .join()
        .expect("a linha do dispositivo não entra em pânico");
    for worker in load {
        worker.join().expect("as linhas de carga terminam");
    }

    println!(
        "{valid} de {total} amostras válidas sob carga; erro máximo {worst_shown:.2} ms no tempo \
         audível, {worst_newest:.2} ms seguindo o bloco mais novo"
    );

    // A perda vem primeiro porque ela invalida o resto: sem o bloco que contém o quadro
    // audível, o consumidor mostra o bloco anterior, e o erro medido passa a ser o tamanho do
    // buraco. Meio segundo de histórico só estoura se o consumidor ficar meio segundo sem
    // processador, e aí a corrida mediu a pausa do agendador, não o modelo.
    assert_eq!(
        viewer.receiver.dropped(),
        0,
        "o consumidor ficou para trás e o barramento descartou blocos"
    );
    assert!(
        f64::from(valid) >= f64::from(total) * MIN_VALID,
        "só {valid} de {total} leituras foram confiáveis: a máquina não sustentou a medição"
    );
    assert!(
        worst_shown < BUDGET_MS,
        "o consumidor mostrou som de {worst_shown:.2} ms fora do tempo audível, acima do \
         orçamento de RF-620"
    );
    assert!(
        worst_newest > worst_shown,
        "seguir o bloco mais novo errou {worst_newest:.2} ms, não mais que os {worst_shown:.2} \
         ms do tempo audível — a medida não está medindo nada"
    );
}

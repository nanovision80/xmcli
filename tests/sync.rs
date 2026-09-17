//! Erro de sincronismo entre o que soa e o que a interface enxerga (RF-620).
//!
//! O alvo é menos de um quadro de vídeo, ≈ 16 ms. Um dispositivo de verdade não serve de
//! medida aqui: o contêiner de integração contínua não tem placa de som, e o erro que se quer
//! medir não é o do hardware, é o do modelo. Então o teste simula um dispositivo que consome
//! em ritmo constante e compara a posição informada pelo relógio com a posição verdadeira.
//!
//! Nada disto roda em tempo real de verdade: o sistema pode tirar qualquer uma das duas
//! linhas de execução do processador por dezenas de milissegundos. Uma pausa dessas atrasa a
//! entrega simulada ou a própria leitura, e o que ela mede é o agendador, não o relógio. Por
//! isso cada amostra é validada antes de contar (ver [`Sample`]), e o teste exige que a
//! maioria delas valha — em uma máquina onde quase nada vale, a medida não significa nada e o
//! teste falha dizendo isso. A medição contra hardware real fica para a etapa 11.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use xmcli::audio::clock::Clock;

const RATE: u32 = 44_100;

/// Quadros por entrega do dispositivo simulado.
///
/// 882 quadros a 44,1 kHz são 20 ms: mais que o alvo de 16 ms de propósito, para que a medida
/// sem compensação estoure o orçamento e a medida com compensação tenha de caber nele.
const BUFFER_FRAMES: u64 = 882;

/// Entregas da corrida — 40 × 20 ms, pouco menos de um segundo de medição.
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

/// Fração mínima de amostras válidas para a corrida valer como medição.
const MIN_VALID: f64 = 0.8;

/// Duração de um buffer, que é o período entre entregas do dispositivo simulado.
fn buffer_period() -> Duration {
    Duration::from_secs_f64(BUFFER_FRAMES as f64 / f64::from(RATE))
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

/// Uma leitura da posição, com o erro das duas formas de calculá-la.
struct Sample {
    compensated_ms: f64,
    naive_ms: f64,
}

/// Lê a posição e compara com a verdade, ou devolve `None` se a leitura não for confiável.
///
/// Confiável é: a leitura coube na tolerância, e o dispositivo simulado já tinha entregue
/// tudo que o horário mandava. Fora disso o que se mediria é a pausa do agendador.
fn sample(clock: &Clock, start: Instant) -> Option<Sample> {
    let before = Instant::now();
    let audible = clock.audible_frame();
    let played = clock.frames_played();
    let after = Instant::now();

    let window = after.duration_since(before);
    if window > TOLERANCE {
        return None;
    }

    // O ponto médio da janela é o instante a que as duas leituras se referem.
    let elapsed = before.duration_since(start) + window / 2;
    let due = elapsed.as_secs_f64() / buffer_period().as_secs_f64();
    if played < (due as u64 + 1) * BUFFER_FRAMES {
        return None;
    }

    let truth = frames_in(elapsed);
    Some(Sample {
        compensated_ms: error_ms(audible, truth),
        naive_ms: error_ms(played, truth),
    })
}

#[test]
fn a_compensacao_de_latencia_cabe_no_orcamento_de_um_quadro() {
    let clock = Clock::new(RATE);
    let running = Arc::new(AtomicBool::new(true));

    let device_clock = Arc::clone(&clock);
    let device_running = Arc::clone(&running);
    // A régua verdadeira nasce na primeira entrega, e não aqui: o atraso da criação da linha
    // de execução mediria o agendador do sistema, não o modelo de latência.
    let (started, start) = std::sync::mpsc::channel();
    let device = std::thread::spawn(move || {
        let start = Instant::now();
        started.send(start).expect("o teste espera pelo início");
        for delivery in 0..DELIVERIES {
            spin_until(start + buffer_period().mul_f64(delivery as f64));
            device_clock.deliver(BUFFER_FRAMES);
        }
        device_running.store(false, Ordering::Release);
    });

    let start = start.recv().expect("a linha do dispositivo começou");

    // O lado da interface: pergunta a posição como faria a cada quadro, só que mais vezes.
    let mut worst_compensated = 0.0_f64;
    let mut worst_naive = 0.0_f64;
    let mut valid = 0_u32;
    let mut total = 0_u32;
    let mut next = start;
    while running.load(Ordering::Acquire) {
        spin_until(next);
        next += SAMPLE_PERIOD;
        total += 1;

        let Some(sample) = sample(&clock, start) else {
            continue;
        };
        worst_compensated = worst_compensated.max(sample.compensated_ms);
        worst_naive = worst_naive.max(sample.naive_ms);
        valid += 1;
    }

    device
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

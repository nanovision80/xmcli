//! Relógio de reprodução e compensação de latência (RF-406, RF-620).
//!
//! A conta que importa não é quantas amostras foram mixadas, e sim quantas o dispositivo já
//! **tornou audíveis**: entre uma coisa e outra existe o buffer, e é essa diferença que faz a
//! imagem adiantar em relação ao som.
//!
//! São duas contas distintas, e confundi-las é o defeito que RF-620 descreve:
//!
//! * [`Clock::frames_played`] — quadros **entregues** ao dispositivo. Anda aos saltos, um
//!   buffer por callback. Serve para saber quando a música acabou de sair.
//! * [`Clock::audible_frame`] — quadro que está **soando agora**. Anda continuamente, porque
//!   interpola entre um callback e o seguinte. É o que toda a interface consome.
//!
//! Sem a interpolação o erro seria de até um buffer inteiro — com a latência padrão, o dobro
//! do alvo de 16 ms. A compensação modela o que o dispositivo declara ter recebido; latência
//! residual do hardware além do callback não entra aqui, e só pode ser medida contra um
//! dispositivo real (etapa 11).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Nanossegundos em um segundo, para converter tempo decorrido em quadros.
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Contagem compartilhada entre a linha de áudio e o resto do programa.
///
/// Escrita só pelo callback do dispositivo, lida por todo mundo.
#[derive(Debug)]
pub struct Clock {
    sample_rate: u32,
    origin: Instant,
    frames_played: AtomicU64,
    /// Instante em que o quadro 0 soou, em nanossegundos desde `origin`.
    ///
    /// É a âncora da reta `audível(t) = (t − âncora) × taxa`. Cada entrega a recalcula, e é
    /// assim que o relógio segue o ritmo real do dispositivo em vez de um ritmo suposto.
    ///
    /// Guardar a âncora, e não o par (instante, contador) da última entrega, é o que dispensa
    /// ler dois valores de uma vez só: um par lido pela metade — carimbo novo com contador
    /// velho — erra um buffer inteiro, justamente o erro que a compensação existe para
    /// eliminar. A âncora é coerente sozinha, e uma âncora atrasada descreve a mesma reta.
    anchor_nanos: AtomicU64,
    underruns: AtomicU64,
    finished: AtomicBool,
    /// O dispositivo está tocando silêncio no lugar da música.
    ///
    /// Escrito pela linha alimentadora, que recebe o pedido da interface pela fila de
    /// comandos; lido pelo callback. Pausado, o callback não consome nem entrega nada, então
    /// a contagem para e o audível fica no que já foi entregue.
    paused: AtomicBool,
}

impl Clock {
    pub fn new(sample_rate: u32) -> Arc<Self> {
        Arc::new(Self {
            sample_rate,
            origin: Instant::now(),
            frames_played: AtomicU64::new(0),
            anchor_nanos: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            paused: AtomicBool::new(false),
        })
    }

    /// Registra quadros entregues ao dispositivo. Chamado de dentro do callback.
    ///
    /// Não aloca, não bloqueia e não pode falhar: são dois `store` e um `Instant::now`
    /// (CLAUDE.md §2, invariante 1). O acúmulo lê o próprio valor anterior sem operação
    /// atômica de leitura-modificação-escrita porque o escritor é único — o callback.
    pub fn deliver(&self, frames: u64) {
        let base = self.frames_played.load(Ordering::Relaxed);
        let nanos = u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX);

        // O primeiro quadro deste buffer começa a soar agora; logo, o quadro 0 soou há o
        // tempo que `base` quadros levam para tocar.
        let played_nanos = u128::from(base) * NANOS_PER_SECOND / u128::from(self.sample_rate);
        let anchor = nanos.saturating_sub(u64::try_from(played_nanos).unwrap_or(u64::MAX));

        self.anchor_nanos.store(anchor, Ordering::Release);
        self.frames_played.store(base + frames, Ordering::Release);
    }

    /// Registra que o callback pediu mais do que havia pronto (RF-402).
    pub fn record_underrun(&self, frames: u64) {
        self.underruns.fetch_add(frames, Ordering::Relaxed);
    }

    /// Marca que o motor chegou ao fim da música e não produzirá mais nada.
    pub fn mark_finished(&self) {
        self.finished.store(true, Ordering::Release);
    }

    /// Quadros já entregues ao dispositivo.
    pub fn frames_played(&self) -> u64 {
        self.frames_played.load(Ordering::Acquire)
    }

    /// Quadro que está soando neste instante (RF-620).
    ///
    /// O dispositivo consome em ritmo constante desde a âncora, então o quadro que soa é o
    /// tempo decorrido desde ela, em quadros. O resultado nunca passa do que já foi entregue:
    /// adiantar a imagem é exatamente o defeito que se quer evitar, e o teto também cobre o
    /// caso de ainda não ter havido entrega alguma.
    ///
    /// Os dois valores são lidos sem coordenação, e não precisam dela: o contador só serve de
    /// teto, e uma entrega acontecendo no meio da leitura leva ao mesmo número por qualquer
    /// um dos dois caminhos.
    pub fn audible_frame(&self) -> u64 {
        let anchor = self.anchor_nanos.load(Ordering::Acquire);
        let played = self.frames_played.load(Ordering::Acquire);

        let frames = self
            .origin
            .elapsed()
            .as_nanos()
            .saturating_sub(u128::from(anchor))
            .saturating_mul(u128::from(self.sample_rate))
            / NANOS_PER_SECOND;

        u64::try_from(frames).unwrap_or(u64::MAX).min(played)
    }

    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const RATE: u32 = 44_100;

    #[test]
    fn contagem_acumula_e_o_fim_e_visivel() {
        let clock = Clock::new(RATE);
        clock.deliver(128);
        clock.deliver(128);
        clock.record_underrun(64);
        assert_eq!(clock.frames_played(), 256);
        assert_eq!(clock.underruns(), 64);

        assert!(!clock.is_finished());
        clock.mark_finished();
        assert!(clock.is_finished());
    }

    #[test]
    fn antes_da_primeira_entrega_nada_soa() {
        let clock = Clock::new(RATE);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(clock.audible_frame(), 0);
    }

    #[test]
    fn a_entrega_recem_feita_ainda_nao_soou() {
        const BUFFER: u64 = 1_024;

        let clock = Clock::new(RATE);
        clock.deliver(BUFFER);

        // O buffer acabou de ser entregue: o que soa é o seu primeiro quadro, não o último.
        assert!(clock.audible_frame() < BUFFER);
        assert_eq!(clock.frames_played(), BUFFER);
    }

    #[test]
    fn o_audivel_avanca_entre_entregas() {
        const BUFFER: u64 = 44_100;
        const WAITED: Duration = Duration::from_millis(20);

        let clock = Clock::new(RATE);
        clock.deliver(BUFFER);
        let start = clock.audible_frame();
        std::thread::sleep(WAITED);
        let after = clock.audible_frame();

        // Um segundo de buffer entregue: 20 ms depois, o audível andou sem nova entrega. A
        // banda é larga porque `sleep` dorme o quanto quiser além do pedido; a precisão da
        // compensação é medida em `tests/sync.rs`, não aqui.
        let advanced = after - start;
        let expected = u64::from(RATE) * WAITED.as_millis() as u64 / 1_000;
        assert!(
            advanced >= expected / 2,
            "audível andou {advanced} quadros, esperado ao menos metade de {expected}"
        );
    }

    #[test]
    fn o_audivel_nunca_passa_do_entregue() {
        const BUFFER: u64 = 64;

        let clock = Clock::new(RATE);
        clock.deliver(BUFFER);
        // Buffer curto e espera longa: sem o teto, a interpolação passaria do fim da música.
        std::thread::sleep(Duration::from_millis(50));

        assert_eq!(clock.audible_frame(), BUFFER);
    }
}

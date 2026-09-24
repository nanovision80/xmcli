//! Ritmo dos quadros e degradação por banda (RF-601, RF-621, RF-622).
//!
//! O terminal não diz quanto aguenta; o que dá para medir é quanto o `write` demora. Enquanto o
//! terminal acompanha, o `write` volta logo, porque o sistema guarda os bytes num buffer e
//! devolve o controle. Quando o terminal — ou o ssh, ou o tmux no meio — não dá conta, o buffer
//! enche e o `write` passa a esperar o terminal escoar. Bytes divididos por esse tempo são o
//! throughput real, e só aparecem quando importam.
//!
//! O [`Pacer`] não tem relógio: recebe quanto cada quadro custou e quanto demorou para sair, e
//! devolve quando desenhar o próximo e com quantas cores. Isso o deixa determinístico nos
//! testes, e deixa o relógio com quem desenha.

use std::time::Duration;

use super::color::ColorDepth;
use crate::config::Ui;

/// Quantas partes tem um percentual.
const PERCENT: f64 = 100.0;

/// Quando e como desenhar o próximo quadro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    /// Quanto esperar, contado do início do quadro que acabou de sair.
    pub interval: Duration,
    /// Com quantas cores pintar o próximo quadro.
    pub depth: ColorDepth,
}

/// Decide o ritmo e a profundidade de cor a partir do custo dos quadros já escritos.
#[derive(Debug)]
pub struct Pacer {
    ui: Ui,
    /// A profundidade que o terminal aceita: a cascata nunca sobe acima dela.
    ceiling: ColorDepth,
    depth: ColorDepth,
    /// Médias móveis dos bytes e da duração de cada `write`. As duas médias, e não a média da
    /// razão: um `write` absorvido pelo buffer tem duração quase zero, e a razão dele iria ao
    /// infinito e dominaria a média.
    average: Option<(f64, f64)>,
    /// Quadros seguidos dentro do orçamento.
    calm_frames: u32,
    /// Quantos quadros calmos são precisos para subir um degrau agora.
    recover_after: u32,
    /// A última subida ainda não se provou: se estourar antes de [`Self::recover_after`]
    /// quadros calmos, ela não se sustentou.
    on_trial: bool,
}

impl Pacer {
    pub fn new(ui: &Ui, ceiling: ColorDepth) -> Self {
        Self {
            ui: ui.clone(),
            ceiling,
            depth: ceiling,
            average: None,
            calm_frames: 0,
            recover_after: ui.recover_frames,
            on_trial: false,
        }
    }

    /// Registra um quadro — os bytes que ele custou e quanto o `write` demorou — e decide o
    /// próximo.
    pub fn record(&mut self, bytes: usize, write_time: Duration) -> Pace {
        let bytes = bytes as f64;
        self.measure(bytes, write_time.as_secs_f64());

        let rate = self.allowed_rate();
        if bytes > self.frame_budget(rate) {
            self.degrade();
        } else {
            self.settle();
        }

        let fastest = 1.0 / f64::from(self.ui.fps_max);
        // Nada de piso: se o quadro precisa de mais tempo que o fps mínimo permite, a imagem
        // perde quadros (CLAUDE.md §2, invariante 2), e a cor já desceu para o próximo caber.
        let interval = fastest.max(bytes / rate);
        Pace {
            interval: Duration::from_secs_f64(interval),
            depth: self.depth,
        }
    }

    /// Throughput medido do terminal, em bytes por segundo, se já houve medida.
    pub fn throughput(&self) -> Option<f64> {
        let (bytes, seconds) = self.average?;
        (seconds > 0.0).then(|| bytes / seconds)
    }

    fn measure(&mut self, bytes: f64, seconds: f64) {
        // Quadro vazio não diz nada sobre o terminal: não houve o que escoar.
        if bytes <= 0.0 {
            return;
        }
        let weight = f64::from(self.ui.throughput_smoothing_percent) / PERCENT;
        self.average = Some(match self.average {
            None => (bytes, seconds),
            Some((old_bytes, old_seconds)) => (
                old_bytes + weight * (bytes - old_bytes),
                old_seconds + weight * (seconds - old_seconds),
            ),
        });
    }

    /// Bytes por segundo que a interface se permite: o teto configurado (RNF-07) ou a fração
    /// permitida do que o terminal mostrou aguentar, o que for menor.
    fn allowed_rate(&self) -> f64 {
        let cap = self.ui.max_bytes_per_second as f64;
        let headroom = f64::from(self.ui.throughput_headroom_percent) / PERCENT;
        self.throughput()
            .map_or(cap, |measured| cap.min(measured * headroom))
    }

    /// Quanto um quadro pode custar: o teto por quadro, ou o que a banda permitida escoa no
    /// intervalo do fps mínimo, o que for menor.
    fn frame_budget(&self, rate: f64) -> f64 {
        let per_frame = rate / f64::from(self.ui.fps_min);
        (self.ui.max_frame_bytes as f64).min(per_frame)
    }

    fn degrade(&mut self) {
        self.calm_frames = 0;
        let Some(lower) = self.depth.lower() else {
            return;
        };
        self.depth = lower;
        if self.on_trial {
            // A subida não se sustentou. Subir de novo no mesmo prazo seria oscilar, e cada
            // troca de cor repinta a tela inteira — justamente o que não cabe.
            self.recover_after = self
                .recover_after
                .saturating_mul(2)
                .min(self.ui.recover_frames_max);
            self.on_trial = false;
        }
    }

    fn settle(&mut self) {
        self.calm_frames = self.calm_frames.saturating_add(1);
        if self.calm_frames < self.recover_after {
            return;
        }
        self.calm_frames = 0;
        self.on_trial = false;
        if let Some(higher) = self.depth.raise().filter(|&higher| higher <= self.ceiling) {
            self.depth = higher;
            self.on_trial = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Marquee;

    /// Um `write` que o buffer do sistema absorveu na hora.
    const FAST: Duration = Duration::from_micros(10);

    fn ui() -> Ui {
        Ui {
            theme: String::new(),
            fps_max: 60,
            fps_min: 30,
            max_bytes_per_second: 300_000,
            max_frame_bytes: 20_000,
            throughput_smoothing_percent: 100,
            throughput_headroom_percent: 50,
            recover_frames: 3,
            recover_frames_max: 12,
            marquee: Marquee {
                chars_per_second: 1,
                pause_ms: 0,
            },
        }
    }

    /// Quadros baratos, que o terminal absorve na hora.
    fn calm(pacer: &mut Pacer, frames: u32) -> Pace {
        let mut pace = pacer.record(100, FAST);
        for _ in 1..frames {
            pace = pacer.record(100, FAST);
        }
        pace
    }

    #[test]
    fn terminal_folgado_desenha_no_fps_maximo() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        let pace = pacer.record(1_000, FAST);
        assert_eq!(pace.interval, Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(pace.depth, ColorDepth::TrueColor);
    }

    #[test]
    fn o_teto_de_banda_espaca_os_quadros() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        // 9 000 bytes a 300 000 B/s: 30 ms, mais que os 16,7 ms do fps máximo.
        let pace = pacer.record(9_000, FAST);
        assert_eq!(pace.interval, Duration::from_millis(30));
        assert_eq!(pace.depth, ColorDepth::TrueColor);
    }

    #[test]
    fn write_que_espera_revela_o_throughput_e_limita_o_ritmo() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        // 2 000 bytes em 20 ms: o terminal escoa 100 000 B/s, e metade disso é permitido.
        let pace = pacer.record(2_000, Duration::from_millis(20));
        assert_eq!(pacer.throughput(), Some(100_000.0));
        assert_eq!(pace.interval, Duration::from_millis(40));
    }

    #[test]
    fn quadro_acima_do_orcamento_desce_um_degrau() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        // 300 000 / 30 = 10 000 bytes por quadro no fps mínimo.
        assert_eq!(pacer.record(10_001, FAST).depth, ColorDepth::Ansi256);
        assert_eq!(pacer.record(10_001, FAST).depth, ColorDepth::Ansi16);
        // 16 cores é o chão da cascata.
        assert_eq!(pacer.record(10_001, FAST).depth, ColorDepth::Ansi16);
    }

    #[test]
    fn teto_por_quadro_vale_mesmo_com_banda_sobrando() {
        let generous = Ui {
            max_bytes_per_second: 10_000_000,
            ..ui()
        };
        let mut pacer = Pacer::new(&generous, ColorDepth::TrueColor);
        assert_eq!(pacer.record(20_000, FAST).depth, ColorDepth::TrueColor);
        assert_eq!(pacer.record(20_001, FAST).depth, ColorDepth::Ansi256);
    }

    #[test]
    fn quadros_calmos_devolvem_a_cor_ate_o_teto_do_terminal() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::Ansi256);
        pacer.record(10_001, FAST);
        assert_eq!(calm(&mut pacer, 2).depth, ColorDepth::Ansi16);
        assert_eq!(calm(&mut pacer, 1).depth, ColorDepth::Ansi256);
        // O terminal só aceita 256: não sobe além disso.
        assert_eq!(calm(&mut pacer, 10).depth, ColorDepth::Ansi256);
    }

    #[test]
    fn subida_que_nao_se_sustenta_dobra_a_espera() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        pacer.record(10_001, FAST);
        assert_eq!(calm(&mut pacer, 3).depth, ColorDepth::TrueColor);
        // Estourou logo depois de subir: a próxima subida pede 6 quadros calmos, não 3.
        pacer.record(10_001, FAST);
        assert_eq!(calm(&mut pacer, 5).depth, ColorDepth::Ansi256);
        assert_eq!(calm(&mut pacer, 1).depth, ColorDepth::TrueColor);
    }

    #[test]
    fn a_espera_dobra_so_ate_o_teto() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        pacer.record(10_001, FAST);
        for _ in 0..5 {
            // Sobe depois da espera corrente e estoura em seguida.
            let wait = pacer.recover_after;
            calm(&mut pacer, wait);
            pacer.record(10_001, FAST);
        }
        assert_eq!(pacer.recover_after, 12);
    }

    #[test]
    fn sem_cor_nao_entra_na_cascata() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::Mono);
        assert_eq!(pacer.record(50_000, FAST).depth, ColorDepth::Mono);
        assert_eq!(calm(&mut pacer, 10).depth, ColorDepth::Mono);
    }

    #[test]
    fn quadro_vazio_nao_mexe_na_medida() {
        let mut pacer = Pacer::new(&ui(), ColorDepth::TrueColor);
        pacer.record(2_000, Duration::from_millis(20));
        pacer.record(0, Duration::from_millis(500));
        assert_eq!(pacer.throughput(), Some(100_000.0));
    }
}

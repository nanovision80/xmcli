//! Barramento master: o PCM pós-mix que a interface enxerga (RF-309).
//!
//! É a fonte do osciloscópio, da FFT e do detector de energia. O áudio publica o mesmo
//! trecho que mandou para o dispositivo, carimbado com o quadro em que ele começa; a
//! interface lê o que corresponde ao tempo audível (RF-620).

use crate::bus::queue::{Receiver, Sender};
use crate::player::CHANNELS;

/// Quadros por bloco publicado.
///
/// Um bloco é a menor unidade que a interface consegue posicionar no tempo, então ele precisa
/// ser bem menor que um quadro de vídeo — a 60 fps, 16,7 ms. A 44,1 kHz, 256 quadros dão
/// 5,8 ms: erro de posicionamento em torno de um terço de quadro, com folga para o alvo de
/// RF-620, e um bloco pequeno o bastante para a cópia não pesar.
pub const BLOCK_FRAMES: usize = 256;

/// Amostras por bloco, já contando os dois canais intercalados.
const BLOCK_SAMPLES: usize = BLOCK_FRAMES * CHANNELS;

/// Milissegundos em um segundo, para converter o histórico configurado em quadros.
const MS_PER_SECOND: u64 = 1_000;

/// Um trecho de áudio pós-mix, de tamanho fixo.
///
/// O tamanho é fixo para que publicar não aloque: quem escreve é a linha de áudio
/// (CLAUDE.md §2, invariante 1). O último bloco de uma música raramente vem cheio, por isso
/// `frames` diz quanto do arranjo vale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    frames: usize,
    samples: [f32; BLOCK_SAMPLES],
}

impl Block {
    /// As amostras válidas, estéreo intercalado.
    pub fn samples(&self) -> &[f32] {
        &self.samples[..self.frames * CHANNELS]
    }

    /// Quantos quadros este bloco carrega.
    pub fn frames(&self) -> usize {
        self.frames
    }
}

/// Abre o barramento master com espaço para `history_ms` de áudio.
///
/// O histórico vira capacidade em blocos: é o quanto o áudio pode correr à frente antes de a
/// interface começar a perder trechos. Ele precisa cobrir o buffer do dispositivo, já que a
/// interface consome atrasada de propósito (RF-620).
pub fn channel(sample_rate: u32, history_ms: u16) -> (Sender<Block>, Receiver<Block>) {
    let history_frames = u64::from(sample_rate) * u64::from(history_ms) / MS_PER_SECOND;
    let blocks = history_frames.div_ceil(BLOCK_FRAMES as u64);
    crate::bus::queue::channel(usize::try_from(blocks).unwrap_or(usize::MAX))
}

/// Publica um trecho pós-mix, fatiado em blocos, a partir do quadro `first_frame`.
///
/// `samples` é estéreo intercalado, como tudo no caminho de áudio. Uma sobra menor que um
/// quadro não é publicada: meia amostra não tem posição no tempo.
pub fn publish(sender: &mut Sender<Block>, first_frame: u64, samples: &[f32]) {
    let mut at_frame = first_frame;
    for chunk in samples.chunks(BLOCK_SAMPLES) {
        let frames = chunk.len() / CHANNELS;
        if frames == 0 {
            return;
        }
        let mut block = Block {
            frames,
            samples: [0.0; BLOCK_SAMPLES],
        };
        block.samples[..frames * CHANNELS].copy_from_slice(&chunk[..frames * CHANNELS]);
        sender.send(at_frame, block);
        at_frame += frames as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::queue::Stamped;

    const RATE: u32 = 44_100;

    /// Uma rampa reconhecível: o valor de cada amostra é o seu índice.
    fn ramp(samples: usize) -> Vec<f32> {
        (0..samples).map(|index| index as f32).collect()
    }

    /// Junta o que o consumidor recebe, conferindo que os carimbos são contínuos.
    fn drain(receiver: &mut Receiver<Block>, first_frame: u64) -> Vec<f32> {
        let mut expected_frame = first_frame;
        let mut collected = Vec::new();
        while let Some(Stamped { at_frame, payload }) = receiver.recv() {
            assert_eq!(at_frame, expected_frame, "carimbo contínuo entre blocos");
            expected_frame += payload.frames() as u64;
            collected.extend_from_slice(payload.samples());
        }
        collected
    }

    #[test]
    fn o_trecho_chega_inteiro_e_em_ordem() {
        let (mut sender, mut receiver) = channel(RATE, 500);
        let source = ramp(BLOCK_SAMPLES * 3);
        publish(&mut sender, 0, &source);

        assert_eq!(drain(&mut receiver, 0), source);
        assert_eq!(receiver.dropped(), 0);
    }

    #[test]
    fn o_ultimo_bloco_parcial_nao_leva_silencio_junto() {
        const TAIL_FRAMES: usize = 7;

        let (mut sender, mut receiver) = channel(RATE, 500);
        let source = ramp(BLOCK_SAMPLES + TAIL_FRAMES * CHANNELS);
        publish(&mut sender, 1_000, &source);

        let received = drain(&mut receiver, 1_000);
        assert_eq!(received.len(), source.len());
        assert_eq!(received, source);
    }

    #[test]
    fn sobra_menor_que_um_quadro_nao_vira_bloco() {
        let (mut sender, mut receiver) = channel(RATE, 500);
        // Um canal só: não fecha um quadro estéreo, então não tem posição no tempo.
        publish(&mut sender, 0, &[1.0]);
        assert_eq!(receiver.recv(), None);
    }

    #[test]
    fn a_capacidade_cobre_o_historico_pedido() {
        const HISTORY_MS: u16 = 500;

        let (mut sender, mut receiver) = channel(RATE, HISTORY_MS);
        let history_frames = RATE as usize * HISTORY_MS as usize / MS_PER_SECOND as usize;
        publish(&mut sender, 0, &ramp(history_frames * CHANNELS));

        assert_eq!(
            receiver.dropped(),
            0,
            "meio segundo de áudio cabe sem perda"
        );
        assert_eq!(drain(&mut receiver, 0).len(), history_frames * CHANNELS);
    }

    #[test]
    fn interface_parada_perde_o_novo_e_conta() {
        // Capacidade de um bloco só: o segundo bloco não tem onde entrar.
        let (mut sender, mut receiver) = channel(RATE, 0);
        publish(&mut sender, 0, &ramp(BLOCK_SAMPLES * 2));

        assert_eq!(receiver.dropped(), 1);
        assert_eq!(drain(&mut receiver, 0).len(), BLOCK_SAMPLES);
    }
}

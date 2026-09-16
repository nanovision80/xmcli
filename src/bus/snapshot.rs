//! Snapshot de estado: onde a música está e o que cada canal faz.
//!
//! Alimenta o cromo (posição, tempo), o mixer de canais e as barras. Diferente do barramento
//! master, que carrega som, aqui viaja o *estado* do replayer no instante carimbado.
//!
//! O que cada campo consegue dizer depende do motor. O libopenmpt expõe posição, tempo e VU
//! por canal, mas não a nota nem o instrumento de cada canal — essa é uma das razões de a
//! etapa 8 escrever o motor próprio (CLAUDE.md §6). Os campos que ele não preenche entram
//! quando houver quem os preencha, e não antes.

use crate::bus::queue::{Receiver, Sender};

/// Canais de padrão que cabem em um snapshot.
///
/// Vem do formato mais largo dos quatro: o Impulse Tracker define 64 canais de padrão, e
/// S3M e XM param antes disso. Não é valor sintonizável — é o teto da especificação, e
/// dimensiona um arranjo que precisa existir em tempo de compilação para publicar sem alocar.
pub const MAX_CHANNELS: usize = 64;

/// Nível de saída de um canal, por lado, na faixa 0,0 a 1,0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vu {
    pub left: f32,
    pub right: f32,
}

impl Vu {
    /// Canal em silêncio.
    pub const SILENT: Self = Self {
        left: 0.0,
        right: 0.0,
    };
}

/// Estado do replayer em um instante.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    /// Posição na sequência de reprodução.
    pub order: u16,
    /// Padrão que está tocando, que a sequência aponta.
    pub pattern: u16,
    /// Linha dentro do padrão.
    pub row: u16,
    /// Ticks por linha — o `speed` dos trackers, não uma velocidade em Hz.
    pub speed: u8,
    /// Andamento em batidas por minuto.
    pub bpm: u16,
    /// Vozes soando, contando as que continuam depois do fim da nota (NNA do IT).
    pub voices: u16,
    /// Quantas posições de [`Snapshot::vu`] valem.
    pub channels: u16,
    /// Nível por canal de padrão.
    pub vu: [Vu; MAX_CHANNELS],
}

impl Snapshot {
    /// Estado de quem não tem estado de tracker: nenhum padrão, nenhum canal.
    ///
    /// É o que um motor sem música responde — o gerador de teste, ou o replayer antes do
    /// primeiro tick.
    pub const IDLE: Self = Self {
        order: 0,
        pattern: 0,
        row: 0,
        speed: 0,
        bpm: 0,
        voices: 0,
        channels: 0,
        vu: [Vu::SILENT; MAX_CHANNELS],
    };

    /// Os níveis dos canais que valem.
    pub fn levels(&self) -> &[Vu] {
        &self.vu[..self.channels as usize]
    }
}

/// Abre o canal de snapshots com espaço para `capacity` deles.
///
/// A capacidade é em snapshots, não em milissegundos, porque a taxa de produção segue os
/// ticks da música: ela muda com o BPM e com o speed, e não há conversão fixa para tempo.
pub fn channel(capacity: u16) -> (Sender<Snapshot>, Receiver<Snapshot>) {
    crate::bus::queue::channel(usize::from(capacity))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_estado_ocioso_nao_tem_canal_algum() {
        assert!(Snapshot::IDLE.levels().is_empty());
    }

    #[test]
    fn os_niveis_param_no_numero_de_canais() {
        let mut snapshot = Snapshot {
            channels: 4,
            ..Snapshot::IDLE
        };
        snapshot.vu[3] = Vu {
            left: 1.0,
            right: 0.5,
        };

        assert_eq!(snapshot.levels().len(), 4);
        assert_eq!(snapshot.levels()[3].left, 1.0);
    }
}

//! Fila SPSC carimbada, a primitiva de que todos os canais do barramento são feitos.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

/// Conteúdo mais o quadro em que ele ocorre.
///
/// O carimbo é o índice do quadro contado desde o início da reprodução, na mesma régua do
/// relógio de [`crate::audio::clock`]. É ele que permite à interface mostrar o que está
/// *soando* em vez do que acabou de ser mixado (RF-620).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamped<T> {
    pub at_frame: u64,
    pub payload: T,
}

/// Cria um canal do barramento com espaço para `capacity` mensagens.
///
/// A capacidade é fixa: o produtor é a linha de áudio, e alocar depois da partida violaria o
/// invariante 1 do CLAUDE.md §2.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    // Um anel de capacidade zero não aceitaria mensagem nenhuma, e o erro apareceria como
    // "a interface não atualiza" em vez de como falha de configuração.
    let (producer, consumer) = HeapRb::<Stamped<T>>::new(capacity.max(1)).split();
    let dropped = Arc::new(AtomicU64::new(0));
    (
        Sender {
            inner: producer,
            dropped: Arc::clone(&dropped),
        },
        Receiver {
            inner: consumer,
            dropped,
        },
    )
}

/// Lado do áudio. Escreve sem bloquear e sem alocar.
pub struct Sender<T> {
    inner: HeapProd<Stamped<T>>,
    dropped: Arc<AtomicU64>,
}

impl<T> Sender<T> {
    /// Publica `payload` como ocorrendo em `at_frame`.
    ///
    /// Com a fila cheia a mensagem **nova** é descartada e a contagem sobe; o produtor segue
    /// em frente. Descartar a nova, e não a antiga, é o que o modelo SPSC permite sem lock:
    /// só o consumidor pode remover. A interface percebe o atraso por [`Receiver::dropped`]
    /// e se ressincroniza — perder quadro é o preço previsto pelo invariante 2.
    pub fn send(&mut self, at_frame: u64, payload: T) {
        let message = Stamped { at_frame, payload };
        if self.inner.try_push(message).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Lado da interface. Consome no seu próprio ritmo, sem segurar o áudio.
pub struct Receiver<T> {
    inner: HeapCons<Stamped<T>>,
    dropped: Arc<AtomicU64>,
}

impl<T> Receiver<T> {
    /// Retira a mensagem mais antiga ainda não lida, se houver.
    pub fn recv(&mut self) -> Option<Stamped<T>> {
        self.inner.try_pop()
    }

    /// Quantas mensagens o produtor descartou por fila cheia desde o início.
    ///
    /// Cresce só quando o consumidor não deu conta: é a medida de que a interface ficou para
    /// trás, e o gatilho para ela redesenhar do zero em vez de acumular estado velho.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserva_ordem_e_carimbo() {
        let (mut sender, mut receiver) = channel(4);
        sender.send(0, 'a');
        sender.send(256, 'b');

        assert_eq!(
            receiver.recv(),
            Some(Stamped {
                at_frame: 0,
                payload: 'a'
            })
        );
        assert_eq!(
            receiver.recv(),
            Some(Stamped {
                at_frame: 256,
                payload: 'b'
            })
        );
        assert_eq!(receiver.recv(), None);
        assert_eq!(receiver.dropped(), 0);
    }

    #[test]
    fn fila_cheia_descarta_a_nova_e_conta() {
        let (mut sender, mut receiver) = channel(2);
        sender.send(0, 'a');
        sender.send(1, 'b');
        sender.send(2, 'c');

        assert_eq!(receiver.dropped(), 1);
        assert_eq!(receiver.recv().map(|message| message.payload), Some('a'));
        assert_eq!(receiver.recv().map(|message| message.payload), Some('b'));
        assert_eq!(receiver.recv(), None);

        // Com espaço de volta, o canal continua servindo: o descarte não o envenena.
        sender.send(3, 'd');
        assert_eq!(receiver.recv().map(|message| message.payload), Some('d'));
        assert_eq!(receiver.dropped(), 1);
    }

    #[test]
    fn atravessa_linhas_de_execucao() {
        const MESSAGES: u64 = 10_000;

        let (mut sender, mut receiver) = channel(64);
        let producer = std::thread::spawn(move || {
            for frame in 0..MESSAGES {
                sender.send(frame, frame);
            }
        });

        let mut received = 0_u64;
        let mut last = None;
        while received + receiver.dropped() < MESSAGES {
            let Some(message) = receiver.recv() else {
                std::thread::yield_now();
                continue;
            };
            assert_eq!(message.at_frame, message.payload);
            if let Some(last) = last {
                assert!(message.at_frame > last, "a ordem de chegada se manteve");
            }
            last = Some(message.at_frame);
            received += 1;
        }

        producer
            .join()
            .expect("a linha produtora não entra em pânico");
        assert_eq!(received + receiver.dropped(), MESSAGES);
    }
}

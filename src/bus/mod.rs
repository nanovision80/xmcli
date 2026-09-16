//! Barramento de visualização: o áudio escreve, a interface lê (RF-521).
//!
//! Tudo que a interface mostra nasce aqui. As regras que valem para **todos** os canais do
//! barramento, e que vêm dos invariantes do CLAUDE.md §2:
//!
//! 1. **Uma fila por direção, um produtor, um consumidor.** Nada de estado compartilhado
//!    mutável entre as duas linhas de execução (invariante 3).
//! 2. **O lado produtor nunca bloqueia nem aloca.** Quem produz é a linha de áudio; esperar
//!    pela interface seria trocar um quadro perdido por uma amostra perdida, que é exatamente
//!    a troca proibida pelo invariante 2.
//! 3. **Toda mensagem carrega o quadro em que ocorre** ([`Stamped`]), porque a interface
//!    consome o tempo *audível*, não o mais recente produzido (invariante 4, RF-620).

pub mod master;
mod queue;

pub use queue::{Receiver, Sender, Stamped, channel};

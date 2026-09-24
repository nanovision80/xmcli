//! Marquee do título (RF-502).
//!
//! A rolagem clássica: o título que não cabe fica parado no começo, corre até o fim aparecer,
//! fica parado no fim e volta de uma vez ao começo. O ritmo vem do relógio de parede, não da
//! música: pausar a reprodução não congela o título, e um BPM alto não o acelera.

use std::time::Duration;

use crate::config::Marquee;

/// Separa o título do módulo do nome do arquivo.
const SEPARATOR: &str = " -- ";

/// O texto que rola: título do módulo e nome do arquivo, ou só o nome se não houver título.
pub fn text(title: &str, file: &str) -> String {
    if title.trim().is_empty() {
        return file.to_owned();
    }
    format!("{title}{SEPARATOR}{file}")
}

/// Milissegundos em um segundo, para converter a velocidade em caracteres por ms.
const MILLIS_PER_SECOND: u128 = 1_000;

/// Quantos caracteres do começo do título ficam escondidos, `elapsed` depois de ele aparecer.
///
/// `len` é o tamanho do título e `width` o da janela, ambos em caracteres. Título que cabe
/// não rola.
pub fn offset(len: usize, width: usize, elapsed: Duration, marquee: &Marquee) -> usize {
    let travel = len.saturating_sub(width);
    if travel == 0 {
        return 0;
    }
    let speed = u128::from(marquee.chars_per_second);
    let pause = u128::from(marquee.pause_ms);
    // Cada posição da rolagem fica um passo na tela, inclusive a última: sem ela, com pausa
    // zero, o fim do título nunca apareceria inteiro.
    let steps = travel as u128 + 1;
    let scroll = (steps * MILLIS_PER_SECOND).div_ceil(speed);
    let cycle = pause + scroll + pause;

    let now = elapsed.as_millis() % cycle;
    let Some(moving) = now.checked_sub(pause) else {
        return 0;
    };
    let moved = (moving * speed / MILLIS_PER_SECOND).min(travel as u128);
    // `moved` não passa de `travel`, que veio de um `usize`.
    moved as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARQUEE: Marquee = Marquee {
        chars_per_second: 10,
        pause_ms: 1_000,
    };

    fn at(millis: u64) -> usize {
        // Título de 30 caracteres numa janela de 20: dez para rolar, um por 100 ms.
        offset(30, 20, Duration::from_millis(millis), &MARQUEE)
    }

    #[test]
    fn texto_junta_titulo_e_arquivo() {
        assert_eq!(
            text("space debris", "debris.mod"),
            "space debris -- debris.mod"
        );
        assert_eq!(text("  ", "sem-titulo.xm"), "sem-titulo.xm");
    }

    #[test]
    fn titulo_que_cabe_nao_rola() {
        for millis in [0, 1_500, 60_000] {
            let elapsed = Duration::from_millis(millis);
            assert_eq!(offset(20, 20, elapsed, &MARQUEE), 0);
            assert_eq!(offset(5, 20, elapsed, &MARQUEE), 0);
        }
    }

    #[test]
    fn para_no_comeco_antes_de_rolar() {
        assert_eq!(at(0), 0);
        assert_eq!(at(999), 0);
    }

    #[test]
    fn rola_um_caractere_por_passo() {
        assert_eq!(at(1_000), 0);
        assert_eq!(at(1_100), 1);
        assert_eq!(at(1_550), 5);
        assert_eq!(at(1_999), 9);
    }

    #[test]
    fn para_no_fim_com_o_final_do_titulo_a_mostra() {
        assert_eq!(at(2_000), 10);
        assert_eq!(at(3_099), 10);
    }

    #[test]
    fn volta_ao_comeco_e_repete() {
        // Ciclo: 1 s parado, 11 passos de 100 ms, 1 s parado.
        assert_eq!(at(3_100), 0);
        assert_eq!(at(4_200), 1);
        assert_eq!(at(3_100 * 7 + 2_500), 10);
    }

    #[test]
    fn sem_pausa_rola_sem_parar() {
        let marquee = Marquee {
            pause_ms: 0,
            ..MARQUEE
        };
        let offset = |millis| offset(30, 20, Duration::from_millis(millis), &marquee);
        assert_eq!(offset(0), 0);
        assert_eq!(offset(999), 9);
        assert_eq!(offset(1_000), 10);
        assert_eq!(offset(1_100), 0);
    }
}

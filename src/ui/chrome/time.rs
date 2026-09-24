//! O display de tempo do transporte, decorrido ou restante (RF-505).
//!
//! `01:23 / 04:57` ou `-03:34 / 04:57`, como no Winamp. Os segundos são truncados dos dois
//! lados, de modo que decorrido e restante sempre somam a duração mostrada.

use crate::config::TimeDisplay;

const SECONDS_PER_MINUTE: u64 = 60;

/// Antes do tempo restante.
const REMAINING_SIGN: char = '-';
/// No lugar do sinal, no decorrido: o texto não muda de largura ao alternar.
const ELAPSED_SIGN: char = ' ';
/// Entre a posição e a duração.
const SEPARATOR: &str = " / ";

/// Uma duração como `mm:ss`; minutos passam de dois dígitos quando precisam.
pub fn clock(seconds: f64) -> String {
    clock_padded(whole_seconds(seconds), 0)
}

/// O display inteiro para a posição dada, sempre com a largura de [`width`].
pub fn display(mode: TimeDisplay, position: f64, duration: f64) -> String {
    let total = whole_seconds(duration);
    let elapsed = whole_seconds(position).min(total);
    let (sign, shown) = match mode {
        TimeDisplay::Elapsed => (ELAPSED_SIGN, elapsed),
        TimeDisplay::Remaining => (REMAINING_SIGN, total - elapsed),
    };
    // A posição usa tantos dígitos de minuto quanto a duração, para o texto não mudar de
    // largura no meio da faixa.
    let digits = minute_digits(total);
    format!(
        "{sign}{}{SEPARATOR}{}",
        clock_padded(shown, digits),
        clock_padded(total, digits)
    )
}

/// Largura de [`display`] para uma faixa com esta duração.
pub fn width(duration: f64) -> usize {
    display(TimeDisplay::Elapsed, 0.0, duration).chars().count()
}

fn whole_seconds(seconds: f64) -> u64 {
    // Negativo e NaN viram zero: posição de antes da primeira entrega, duração desconhecida.
    seconds.max(0.0) as u64
}

fn minute_digits(seconds: u64) -> usize {
    (seconds / SECONDS_PER_MINUTE).to_string().len()
}

/// `mm:ss`, com os minutos em pelo menos dois dígitos ou em `digits`, o que for maior.
fn clock_padded(seconds: u64, digits: usize) -> String {
    const MIN_MINUTE_DIGITS: usize = 2;
    let digits = digits.max(MIN_MINUTE_DIGITS);
    format!(
        "{:0digits$}:{:02}",
        seconds / SECONDS_PER_MINUTE,
        seconds % SECONDS_PER_MINUTE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duracao_em_minutos_e_segundos() {
        assert_eq!(clock(0.0), "00:00");
        assert_eq!(clock(297.9), "04:57");
        assert_eq!(clock(6_000.0), "100:00");
        assert_eq!(clock(-1.0), "00:00");
    }

    #[test]
    fn decorrido_e_restante_somam_a_duracao() {
        assert_eq!(display(TimeDisplay::Elapsed, 83.6, 297.9), " 01:23 / 04:57");
        assert_eq!(
            display(TimeDisplay::Remaining, 83.6, 297.9),
            "-03:34 / 04:57"
        );
        assert_eq!(
            display(TimeDisplay::Remaining, 0.0, 297.9),
            "-04:57 / 04:57"
        );
    }

    #[test]
    fn posicao_alem_do_fim_para_no_fim() {
        assert_eq!(
            display(TimeDisplay::Elapsed, 400.0, 297.0),
            " 04:57 / 04:57"
        );
        assert_eq!(
            display(TimeDisplay::Remaining, 400.0, 297.0),
            "-00:00 / 04:57"
        );
    }

    #[test]
    fn a_largura_nao_muda_na_faixa_nem_ao_alternar() {
        let duration = 6_000.0;
        let expected = width(duration);
        for position in [0.0, 59.0, 600.0, 5_999.0] {
            for mode in [TimeDisplay::Elapsed, TimeDisplay::Remaining] {
                assert_eq!(display(mode, position, duration).chars().count(), expected);
            }
        }
        assert_eq!(
            display(TimeDisplay::Elapsed, 5.0, duration),
            " 000:05 / 100:00"
        );
    }
}

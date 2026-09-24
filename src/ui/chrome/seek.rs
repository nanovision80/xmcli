//! A barra de seek: onde a música está dentro da faixa (RF-504).
//!
//! `[=======o--------]`: o que já tocou, o ponto atual e o que falta. Só ASCII, e a posição
//! aparece pela forma, sem depender de cor.

/// Pontas da barra.
const OPEN: char = '[';
const CLOSE: char = ']';
/// O que já tocou.
const PLAYED: char = '=';
/// O ponto atual.
const KNOB: char = 'o';
/// O que falta tocar.
const AHEAD: char = '-';

/// A barra pronta para desenhar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    pub text: String,
    /// Em que caractere de `text` está o ponto atual, para quem quiser destacá-lo.
    pub knob: usize,
}

/// A barra com `width` caracteres para a posição dada; `None` se não couber nem o ponto.
///
/// Duração desconhecida ou zero deixa o ponto no começo: sem duração não há proporção.
pub fn bar(width: usize, position: f64, duration: f64) -> Option<Bar> {
    // As pontas mais o ponto.
    let track = width.checked_sub(2).filter(|&track| track > 0)?;
    let fraction = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // O ponto anda de ponta a ponta: no começo da faixa encosta no `[`, no fim no `]`.
    let knob = (fraction * (track - 1) as f64).round() as usize;

    let mut text = String::with_capacity(width);
    text.push(OPEN);
    text.extend(std::iter::repeat_n(PLAYED, knob));
    text.push(KNOB);
    text.extend(std::iter::repeat_n(AHEAD, track - 1 - knob));
    text.push(CLOSE);
    Some(Bar {
        text,
        knob: knob + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(width: usize, position: f64, duration: f64) -> String {
        bar(width, position, duration).expect("cabe").text
    }

    #[test]
    fn o_ponto_anda_de_ponta_a_ponta() {
        assert_eq!(text(12, 0.0, 100.0), "[o---------]");
        assert_eq!(text(12, 50.0, 100.0), "[=====o----]");
        assert_eq!(text(12, 100.0, 100.0), "[=========o]");
    }

    #[test]
    fn a_barra_tem_sempre_a_largura_pedida() {
        for position in [0.0, 13.0, 77.7, 100.0, 250.0, -5.0] {
            assert_eq!(text(40, position, 100.0).chars().count(), 40);
        }
    }

    #[test]
    fn fora_da_faixa_fica_nas_pontas() {
        assert_eq!(text(7, -3.0, 10.0), "[o----]");
        assert_eq!(text(7, 30.0, 10.0), "[====o]");
    }

    #[test]
    fn sem_duracao_o_ponto_fica_no_comeco() {
        assert_eq!(text(7, 5.0, 0.0), "[o----]");
    }

    #[test]
    fn o_indice_do_ponto_aponta_para_ele() {
        let bar = bar(20, 33.0, 100.0).expect("cabe");
        assert_eq!(bar.text.chars().nth(bar.knob), Some(KNOB));
    }

    #[test]
    fn largura_sem_espaco_para_o_ponto_nao_desenha() {
        assert_eq!(bar(2, 0.0, 1.0), None);
        assert_eq!(bar(0, 0.0, 1.0), None);
        assert!(bar(3, 0.0, 1.0).is_some());
    }
}

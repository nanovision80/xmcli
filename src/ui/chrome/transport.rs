//! Os botões do transporte, com o do estado atual destacado (RF-503).
//!
//! O destaque é cor **e** forma: o botão ativo ganha parênteses, porque sem cor — `--mono`,
//! `NO_COLOR` — só a forma diz qual é.

use crate::ui::transport::Transport;

/// Os botões, na ordem do Winamp: anterior, tocar, pausar, parar, próxima.
const BUTTONS: [(&str, Option<Transport>); 5] = [
    ("|<", None),
    (">", Some(Transport::Playing)),
    ("||", Some(Transport::Paused)),
    ("[]", Some(Transport::Stopped)),
    (">|", None),
];

/// Em volta do botão ativo.
const ACTIVE_OPEN: char = '(';
const ACTIVE_CLOSE: char = ')';

/// Em volta dos outros, para todos ocuparem o mesmo lugar em qualquer estado.
const IDLE_EDGE: char = ' ';

/// Entre um botão e o seguinte.
const GAP: &str = " ";

/// Um trecho da linha do transporte, e se ele é o botão ativo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub active: bool,
}

/// A linha de botões para o estado dado, em trechos na ordem em que aparecem.
pub fn buttons(state: Transport) -> Vec<Piece> {
    let mut pieces = Vec::with_capacity(BUTTONS.len() * 2);
    for (index, (label, shows)) in BUTTONS.iter().enumerate() {
        if index > 0 {
            pieces.push(Piece {
                text: GAP.to_owned(),
                active: false,
            });
        }
        let active = *shows == Some(state);
        let (open, close) = if active {
            (ACTIVE_OPEN, ACTIVE_CLOSE)
        } else {
            (IDLE_EDGE, IDLE_EDGE)
        };
        pieces.push(Piece {
            text: format!("{open}{label}{close}"),
            active,
        });
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(state: Transport) -> String {
        buttons(state).into_iter().map(|piece| piece.text).collect()
    }

    fn active(state: Transport) -> Vec<String> {
        buttons(state)
            .into_iter()
            .filter(|piece| piece.active)
            .map(|piece| piece.text)
            .collect()
    }

    #[test]
    fn o_botao_do_estado_ganha_parenteses() {
        assert_eq!(line(Transport::Playing), " |<  (>)  ||   []   >| ");
        assert_eq!(line(Transport::Paused), " |<   >  (||)  []   >| ");
        assert_eq!(line(Transport::Stopped), " |<   >   ||  ([])  >| ");
    }

    #[test]
    fn so_um_botao_fica_ativo() {
        assert_eq!(active(Transport::Playing), ["(>)"]);
        assert_eq!(active(Transport::Paused), ["(||)"]);
        assert_eq!(active(Transport::Stopped), ["([])"]);
    }

    #[test]
    fn a_linha_nao_muda_de_largura_com_o_estado() {
        let width = |state| line(state).chars().count();
        assert_eq!(width(Transport::Playing), width(Transport::Paused));
        assert_eq!(width(Transport::Playing), width(Transport::Stopped));
    }
}

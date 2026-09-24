//! O cromo do player: moldura, cabeçalho, transporte, seek e status (RF-500).
//!
//! Cada parte entra com o item do roteiro que a desenha; por enquanto, o layout, a moldura, o
//! marquee do título, os botões do transporte e a barra de seek.

pub mod frame;
pub mod layout;
pub mod marquee;
pub mod seek;
pub mod transport;

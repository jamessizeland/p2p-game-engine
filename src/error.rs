#![allow(unused)]

use crate::GameLogic;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError<G: GameLogic> {
    #[error("Game logic error: {0}")]
    Game(#[from] G::GameError),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Invalid action: {0}")]
    InvalidAction(String),

    #[error("State parsing error: {0}")]
    StateParse(String),
}

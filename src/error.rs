#![allow(unused)]

use thiserror::Error;

#[derive(Error, PartialEq, Debug)]
pub enum Error {
    #[error("unknown error.")]
    Unknown,
    #[error("No Game State found.")]
    NoGameStateFound,
    #[error("No App State found.")]
    NoAppStateFound,
}

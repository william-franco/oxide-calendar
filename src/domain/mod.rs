//! Pure domain: dates, recurrence, events, layout and search. No UI and no database.

pub mod category;
pub mod datetime;
pub mod event;
pub mod layout;
pub mod recurrence;
pub mod search;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    InvalidRule(&'static str),
    InvalidEvent(&'static str),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRule(msg) => write!(f, "Regra de recorrência inválida: {msg}"),
            Self::InvalidEvent(msg) => write!(f, "Evento inválido: {msg}"),
        }
    }
}

impl std::error::Error for DomainError {}

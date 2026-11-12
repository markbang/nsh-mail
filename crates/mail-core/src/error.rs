//! Conversions from library errors into the crate's serializable `Error`.

use crate::types::{Error, ErrorCode};

pub(crate) fn err(code: ErrorCode, message: impl Into<String>) -> Error {
    Error::new(code, message)
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        err(ErrorCode::Storage, format!("database error: {e}"))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        err(ErrorCode::Internal, format!("serialization error: {e}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        err(ErrorCode::Network, format!("I/O error: {e}"))
    }
}

impl From<async_imap::error::Error> for Error {
    fn from(e: async_imap::error::Error) -> Self {
        use async_imap::error::Error as E;
        match e {
            E::Io(io) => err(ErrorCode::Network, format!("IMAP connection error: {io}")),
            E::No(msg) => err(ErrorCode::Protocol, format!("IMAP server refused: {msg}")),
            E::Bad(msg) => err(
                ErrorCode::Protocol,
                format!("IMAP server rejected the command: {msg}"),
            ),
            E::ConnectionLost => err(ErrorCode::Network, "IMAP connection lost"),
            other => err(ErrorCode::Protocol, format!("IMAP error: {other}")),
        }
    }
}

impl From<lettre::transport::smtp::Error> for Error {
    fn from(e: lettre::transport::smtp::Error) -> Self {
        let code =
            if e.is_permanent() && e.status().is_some_and(|c| c.to_string().starts_with("53")) {
                ErrorCode::Auth
            } else if e.is_permanent() || e.is_transient() {
                ErrorCode::Protocol
            } else {
                ErrorCode::Network
            };
        err(code, format!("SMTP error: {e}"))
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        err(ErrorCode::Network, format!("HTTP error: {e}"))
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        err(ErrorCode::Internal, format!("background task failed: {e}"))
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        err(ErrorCode::Network, "the mail server did not answer in time")
    }
}

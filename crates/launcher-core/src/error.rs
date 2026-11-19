use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("download of {url} failed verification (expected sha1 {expected}, got {actual})")]
    HashMismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("sign-in failed: {0}")]
    Auth(String),
    #[error("not signed in")]
    NotSignedIn,
    #[error("unsafe path in pack manifest: {0}")]
    UnsafePath(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    /// A short, non-identifying category for anonymous telemetry (never the message,
    /// which can contain paths or names).
    pub fn category(&self) -> &'static str {
        match self {
            Error::Http(e) if e.is_timeout() => "network_timeout",
            Error::Http(e) if e.is_connect() => "network_connect",
            Error::Http(_) => "network",
            Error::Io(_) => "file",
            Error::Json(_) => "invalid_data",
            Error::HashMismatch { .. } => "hash_mismatch",
            Error::Auth(_) => "auth",
            Error::NotSignedIn => "not_signed_in",
            Error::UnsafePath(_) => "unsafe_path",
            Error::Other(_) => "other",
        }
    }
}

// Lets Tauri commands return `Result<T, Error>` directly; the UI gets the message.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

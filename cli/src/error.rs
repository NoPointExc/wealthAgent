use std::fmt;

#[derive(Debug)]
pub enum ClientError {
    Auth(String),
    NotFound(String),
    Server(String),
    Network(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(m) => write!(f, "auth error: {m}"),
            Self::NotFound(m) => write!(f, "not found: {m}"),
            Self::Server(m) => write!(f, "server error: {m}"),
            Self::Network(m) => write!(f, "network error: {m}"),
        }
    }
}

impl std::error::Error for ClientError {}

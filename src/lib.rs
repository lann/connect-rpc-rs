pub(crate) mod common;
pub mod metadata;
pub mod request;
pub mod response;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("base64 decode error: {0}")]
    Base64DecodeError(#[from] base64::DecodeError),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    #[error("invalid URI: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error("invalid URI: {0}")]
    InvalidUriParts(#[from] http::uri::InvalidUriParts),
}

impl Error {
    pub(crate) fn invalid_request(msg: impl std::fmt::Display) -> Self {
        Self::InvalidRequest(msg.to_string())
    }
}

pub(crate) mod common;
pub mod metadata;
pub mod request;
pub mod response;
pub mod stream;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("base64 decode error: {0}")]
    Base64DecodeError(#[from] base64::DecodeError),
    #[error("body error: {0}")]
    BodyError(#[source] BoxError),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("invalid metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("invalid header name: {0}")]
    InvalidHeaderName(#[from] http::header::InvalidHeaderName),
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    #[error("invalid URI: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error("invalid URI: {0}")]
    InvalidUriParts(#[from] http::uri::InvalidUriParts),
    #[error("unacceptable encoding {0:?}")]
    UnacceptableEncoding(String),
    #[error("unexpected message codec {0:?}")]
    UnexpectedMessageCodec(String),
}

impl Error {
    pub(crate) fn body(err: impl Into<BoxError>) -> Self {
        Self::BodyError(err.into())
    }

    pub(crate) fn invalid_request(msg: impl std::fmt::Display) -> Self {
        Self::InvalidRequest(msg.to_string())
    }
}

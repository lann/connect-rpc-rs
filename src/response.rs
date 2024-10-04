pub mod builder;

use http::{header, HeaderMap, StatusCode};

use crate::{
    common::{streaming_message_codec, unary_message_codec, CONNECT_CONTENT_ENCODING},
    metadata::Metadata,
    Error,
};

/// A Connect response.
pub trait ConnectResponse {
    /// Returns the status code.
    fn status(&self) -> StatusCode;

    /// Returns the message codec.
    fn message_codec(&self) -> Result<&str, Error>;

    /// Returns the content encoding.
    fn content_encoding(&self) -> Option<&str>;

    /// Returns a reference to the metadata.
    fn metadata(&self) -> &impl Metadata;
}

trait HttpConnectResponse {
    fn status(&self) -> StatusCode;

    fn headers(&self) -> &HeaderMap;

    fn message_codec(&self) -> Result<&str, Error>;

    fn content_encoding(&self) -> Option<&str>;
}

impl<T: HttpConnectResponse> ConnectResponse for T {
    fn status(&self) -> StatusCode {
        HttpConnectResponse::status(self)
    }

    fn message_codec(&self) -> Result<&str, Error> {
        HttpConnectResponse::message_codec(self)
    }

    fn content_encoding(&self) -> Option<&str> {
        HttpConnectResponse::content_encoding(self)
    }

    fn metadata(&self) -> &impl Metadata {
        self.headers()
    }
}

pub struct UnaryResponse<T>(http::Response<T>);

impl<T> HttpConnectResponse for UnaryResponse<T> {
    fn status(&self) -> StatusCode {
        self.0.status()
    }

    fn headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    fn message_codec(&self) -> Result<&str, Error> {
        unary_message_codec(self.headers())
    }

    fn content_encoding(&self) -> Option<&str> {
        self.headers().get(header::CONTENT_ENCODING)?.to_str().ok()
    }
}

impl<T> From<http::Response<T>> for UnaryResponse<T> {
    fn from(resp: http::Response<T>) -> Self {
        Self(resp)
    }
}

impl<T> From<UnaryResponse<T>> for http::Response<T> {
    fn from(resp: UnaryResponse<T>) -> Self {
        resp.0
    }
}

pub struct StreamingResponse<T>(http::Response<T>);

impl<T> HttpConnectResponse for StreamingResponse<T> {
    fn status(&self) -> StatusCode {
        self.0.status()
    }

    fn headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    fn message_codec(&self) -> Result<&str, Error> {
        streaming_message_codec(self.headers())
    }

    fn content_encoding(&self) -> Option<&str> {
        self.headers().get(CONNECT_CONTENT_ENCODING)?.to_str().ok()
    }
}

impl<T> From<http::Response<T>> for StreamingResponse<T> {
    fn from(resp: http::Response<T>) -> Self {
        Self(resp)
    }
}

impl<T> From<StreamingResponse<T>> for http::Response<T> {
    fn from(resp: StreamingResponse<T>) -> Self {
        resp.0
    }
}

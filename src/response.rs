pub mod builder;
pub mod error;

use error::ConnectError;
use http::{header, HeaderMap, StatusCode};

use crate::{
    common::{
        streaming_message_codec, unary_message_codec, CONNECT_CONTENT_ENCODING,
        CONTENT_ENCODING_IDENTITY,
    },
    metadata::Metadata,
    request::ConnectRequest,
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

    /// Validates the response.
    fn validate(&self, opts: &ValidateOpts) -> Result<(), Error>;
}

/// Options for [`ConnectResponse::validate`].
#[derive(Clone, Debug, Default)]
pub struct ValidateOpts {
    /// If given, the response message codec must match.
    pub message_codec: Option<String>,
    /// If given, the response content encoding must match (or be 'identity').
    pub accept_encoding: Option<Vec<String>>,
}

impl ValidateOpts {
    pub fn from_request(req: &impl ConnectRequest) -> Self {
        let message_codec = req.message_codec().map(ToString::to_string).ok();
        let accept_encoding = Some(req.accept_encoding().map(ToString::to_string).collect());
        Self {
            message_codec,
            accept_encoding,
        }
    }
}

trait HttpConnectResponse {
    fn http_status(&self) -> StatusCode;

    fn http_headers(&self) -> &HeaderMap;

    fn http_message_codec(&self) -> Result<&str, Error>;

    fn http_content_encoding(&self) -> Option<&str>;
}

impl<T: HttpConnectResponse> ConnectResponse for T {
    fn status(&self) -> StatusCode {
        self.http_status()
    }

    fn message_codec(&self) -> Result<&str, Error> {
        self.http_message_codec()
    }

    fn content_encoding(&self) -> Option<&str> {
        self.http_content_encoding()
    }

    fn metadata(&self) -> &impl Metadata {
        self.http_headers()
    }

    fn validate(&self, opts: &ValidateOpts) -> Result<(), Error> {
        let codec = self.message_codec()?;
        if let Some(validate_codec) = &opts.message_codec {
            if codec != validate_codec {
                return Err(Error::UnexpectedMessageCodec(codec.into()));
            }
        }
        if let Some(encoding) = self.content_encoding() {
            if encoding != CONTENT_ENCODING_IDENTITY {
                if let Some(accept_encoding) = &opts.accept_encoding {
                    if !accept_encoding.iter().any(|accept| accept == encoding) {
                        return Err(Error::UnacceptableEncoding(encoding.into()));
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct UnaryResponse<T>(http::Response<T>);

impl<T> UnaryResponse<T> {
    pub fn body(&self) -> &T {
        self.0.body()
    }
}

impl<T: AsRef<[u8]>> UnaryResponse<T> {
    pub fn error(&self, validate_opts: &ValidateOpts) -> Option<ConnectError> {
        if let Some(error) = ConnectError::from_http(&self.0) {
            Some(error)
        } else if let Err(err) = self.validate(validate_opts) {
            tracing::debug!(?err, "Invalid response");
            Some(err.into())
        } else {
            None
        }
    }
}

impl<T> HttpConnectResponse for UnaryResponse<T> {
    fn http_status(&self) -> StatusCode {
        self.0.status()
    }

    fn http_headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    fn http_message_codec(&self) -> Result<&str, Error> {
        unary_message_codec(self.http_headers())
    }

    fn http_content_encoding(&self) -> Option<&str> {
        self.http_headers()
            .get(header::CONTENT_ENCODING)?
            .to_str()
            .ok()
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
    fn http_status(&self) -> StatusCode {
        self.0.status()
    }

    fn http_headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    fn http_message_codec(&self) -> Result<&str, Error> {
        streaming_message_codec(self.http_headers())
    }

    fn http_content_encoding(&self) -> Option<&str> {
        self.http_headers()
            .get(CONNECT_CONTENT_ENCODING)?
            .to_str()
            .ok()
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

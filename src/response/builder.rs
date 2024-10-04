use http::{header, HeaderMap, StatusCode};

use crate::{
    common::{is_valid_http_token, CONNECT_CONTENT_ENCODING, CONTENT_TYPE_PREFIX},
    metadata::Metadata,
    Error,
};

use super::{StreamingResponse, UnaryResponse};

#[derive(Debug, Default)]
pub struct ResponseBuilder {
    status: StatusCode,
    metadata: HeaderMap,
    message_codec: Option<String>,
    content_encoding: Option<String>,
}

impl ResponseBuilder {
    /// Sets the response status code.
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Gets a mutable reference to the response [`Metadata`].
    pub fn metadata_mut(&mut self) -> &mut impl Metadata {
        &mut self.metadata
    }

    /// Sets the message codec for this response.
    ///
    /// Typical codecs are 'json' and 'proto', corresponding to the
    /// `content-type`s `application/json` and `application/proto`.
    ///
    /// The caller is responsible for making sure the response payload matches
    /// this message codec.
    pub fn message_codec(mut self, message_codec: impl Into<String>) -> Result<Self, Error> {
        let mut message_codec: String = message_codec.into();
        message_codec.make_ascii_lowercase();
        if !is_valid_http_token(&message_codec) {
            return Err(Error::invalid_request("invalid message codec"));
        }
        self.message_codec = Some(message_codec);
        Ok(self)
    }

    /// Sets the response content encoding (e.g. compression).
    pub fn content_encoding(mut self, content_encoding: impl Into<String>) -> Result<Self, Error> {
        let content_encoding = content_encoding.into();
        if !is_valid_http_token(&content_encoding) {
            return Err(Error::invalid_request("invalid content encoding"));
        }
        self.content_encoding = Some(content_encoding);
        Ok(self)
    }

    /// Build logic common to all responses.
    fn common_response<T>(&mut self, body: T) -> http::Response<T> {
        let mut resp = http::Response::new(body);
        *resp.status_mut() = self.status;
        *resp.headers_mut() = std::mem::take(&mut self.metadata);
        resp
    }

    /// Builds a [`UnaryResponse`].
    pub fn unary<T>(mut self, body: T) -> Result<UnaryResponse<T>, Error> {
        let mut resp = self.common_response(body);
        // Unary-Content-Type → "content-type" "application/" Message-Codec
        if let Some(message_codec) = &self.message_codec {
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                (format!("{CONTENT_TYPE_PREFIX}{message_codec}")).try_into()?,
            );
        }
        // Content-Encoding → "content-encoding" Content-Coding
        if let Some(content_encoding) = self.content_encoding.take() {
            resp.headers_mut()
                .insert(header::CONTENT_ENCODING, content_encoding.try_into()?);
        }
        Ok(resp.into())
    }

    /// Builds a [`StreamingResponse`].
    pub fn streaming<T>(mut self, body: T) -> Result<StreamingResponse<T>, Error> {
        let mut resp = self.common_response(body);
        // Streaming-Content-Type → "content-type" "application/connect+" [...]
        if let Some(message_codec) = &self.message_codec {
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                (format!("{CONTENT_TYPE_PREFIX}{message_codec}")).try_into()?,
            );
        }
        // Streaming-Content-Encoding → "connect-content-encoding" Content-Coding
        if let Some(content_encoding) = self.content_encoding.take() {
            resp.headers_mut()
                .insert(CONNECT_CONTENT_ENCODING, content_encoding.try_into()?);
        }
        Ok(resp.into())
    }
}

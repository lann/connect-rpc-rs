use base64::prelude::{Engine, BASE64_STANDARD_NO_PAD};
use http::{header, HeaderMap, HeaderName, HeaderValue};

use crate::Error;

pub const CONNECT_PROTOCOL_VERSION: HeaderName =
    HeaderName::from_static("connect-protocol-version");
pub const PROTOCOL_VERSION_1: HeaderValue = HeaderValue::from_static("1");

pub const CONNECT_TIMEOUT_MS: HeaderName = HeaderName::from_static("connect-timeout-ms");

pub const CONNECT_CONTENT_ENCODING: HeaderName =
    HeaderName::from_static("connect-content-encoding");
pub const CONNECT_ACCEPT_ENCODING: HeaderName = HeaderName::from_static("connect-accept-encoding");
pub const CONTENT_ENCODING_IDENTITY: HeaderValue = HeaderValue::from_static("identity");

pub const CONTENT_TYPE_PREFIX: &str = "application/";
pub const STREAMING_CONTENT_TYPE_PREFIX: &str = "application/connect+";
pub const STREAMING_CONTENT_SUBTYPE_PREFIX: &str = "connect+";

pub fn base64_encode(input: impl AsRef<[u8]>) -> String {
    BASE64_STANDARD_NO_PAD.encode(input)
}

pub fn base64_decode(b64: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
    Ok(BASE64_STANDARD_NO_PAD.decode(b64)?)
}

pub fn is_valid_http_token(s: &str) -> bool {
    // https://httpwg.org/http-core/draft-ietf-httpbis-semantics-latest.html#tokens
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

pub fn unary_message_codec(headers: &HeaderMap) -> Result<&str, Error> {
    let codec = content_type(headers)?
        .strip_prefix(CONTENT_TYPE_PREFIX)
        .ok_or(Error::invalid_request(
            "content-type must start with 'application/'",
        ))?;
    if codec.starts_with(STREAMING_CONTENT_SUBTYPE_PREFIX) {
        return Err(Error::invalid_request(
            "unary request with streaming content-type",
        ));
    }
    Ok(codec)
}

pub fn streaming_message_codec(headers: &HeaderMap) -> Result<&str, Error> {
    content_type(headers)?
        .strip_prefix(STREAMING_CONTENT_SUBTYPE_PREFIX)
        .ok_or(Error::invalid_request(
            "streaming content-type must start with 'application/connect+'",
        ))
}

fn content_type(headers: &HeaderMap) -> Result<&str, Error> {
    headers
        .get(header::CONTENT_TYPE)
        .ok_or(Error::invalid_request("missing content-type"))?
        .to_str()
        .map_err(|_| Error::invalid_request("invalid content-type"))
}

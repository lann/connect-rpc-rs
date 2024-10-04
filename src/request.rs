use std::{borrow::Cow, collections::HashMap, time::Duration};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE, Engine};
use http::{
    header,
    uri::{Authority, Scheme},
    HeaderMap, Method, Uri,
};

use crate::{
    common::{
        streaming_message_codec, unary_message_codec, CONNECT_ACCEPT_ENCODING,
        CONNECT_CONTENT_ENCODING, CONNECT_PROTOCOL_VERSION, CONNECT_TIMEOUT_MS,
        STREAMING_CONTENT_TYPE_PREFIX,
    },
    metadata::Metadata,
    Error,
};

pub mod builder;

/// A Connect request.
pub trait ConnectRequest {
    /// Returns the connect protocol version.
    fn connect_protocol_version(&self) -> Option<&str>;

    /// Returns the URI scheme.
    fn scheme(&self) -> Option<&Scheme>;

    /// Returns the URI authority.
    fn authority(&self) -> Option<&Authority>;

    /// Returns the URI path.
    fn path(&self) -> &str;

    /// Splits a protobuf RPC request path into routing prefix, service name,
    /// and method name.
    ///
    /// Returns `None` if the request path does not contain a `/`.
    fn protobuf_rpc_parts(&self) -> Option<(&str, &str, &str)> {
        let (prefix, method) = self.path().rsplit_once('/')?;
        let (routing_prefix, service) = prefix.rsplit_once('/')?;
        Some((routing_prefix, service, method))
    }

    /// Returns the message codec.
    fn message_codec(&self) -> Result<&str, Error>;

    /// Returns the timeout.
    fn timeout(&self) -> Option<Duration>;

    /// Returns the content encoding (e.g. compression).
    fn content_encoding(&self) -> Option<&str>;

    /// Returns the accept encoding(s).
    fn accept_encoding(&self) -> impl Iterator<Item = &str>;

    /// Returns the metadata.
    fn metadata(&self) -> &impl Metadata;
}

/// Connect request types.
pub enum ConnectRequestType<T> {
    Unary(UnaryRequest<T>),
    Streaming(StreamingRequest<T>),
    UnaryGet(UnaryGetRequest),
}

impl<T> ConnectRequestType<T> {
    pub fn from_http(req: http::Request<T>) -> Self {
        if req.method() == Method::GET {
            Self::UnaryGet(req.map(|_| ()).into())
        } else if req.headers().get(header::CONTENT_TYPE).is_some_and(|ct| {
            ct.to_str()
                .unwrap_or_default()
                .starts_with(STREAMING_CONTENT_TYPE_PREFIX)
        }) {
            Self::Streaming(req.into())
        } else {
            Self::Unary(req.into())
        }
    }
}

/// A [`ConnectRequest`] backed by an [`http::Request`]
trait HttpConnectRequest {
    fn uri(&self) -> &Uri;

    fn headers(&self) -> &HeaderMap;

    fn message_codec(&self) -> Result<&str, Error>;

    fn connect_protocol_version(&self) -> Option<&str> {
        self.headers().get(CONNECT_PROTOCOL_VERSION)?.to_str().ok()
    }

    fn content_encoding(&self) -> Option<&str>;

    fn accept_encoding(&self) -> impl Iterator<Item = &str> {
        self.headers()
            .get_all(header::ACCEPT_ENCODING)
            .into_iter()
            .filter_map(|val| val.to_str().ok())
    }
}

impl<T: HttpConnectRequest> ConnectRequest for T {
    fn connect_protocol_version(&self) -> Option<&str> {
        HttpConnectRequest::connect_protocol_version(self)
    }

    fn scheme(&self) -> Option<&Scheme> {
        self.uri().scheme()
    }

    fn authority(&self) -> Option<&Authority> {
        self.uri().authority()
    }

    fn path(&self) -> &str {
        self.uri().path()
    }

    fn message_codec(&self) -> Result<&str, Error> {
        HttpConnectRequest::message_codec(self)
    }

    fn timeout(&self) -> Option<Duration> {
        let timeout_ms: u64 = self
            .headers()
            .get(CONNECT_TIMEOUT_MS)?
            .to_str()
            .ok()?
            .parse()
            .ok()?;
        Some(Duration::from_millis(timeout_ms))
    }

    fn content_encoding(&self) -> Option<&str> {
        HttpConnectRequest::content_encoding(self)
    }

    fn accept_encoding(&self) -> impl Iterator<Item = &str> {
        HttpConnectRequest::accept_encoding(self)
    }

    fn metadata(&self) -> &impl Metadata {
        self.headers()
    }
}

/// A Connect unary request.
pub struct UnaryRequest<T>(http::Request<T>);

impl<T> HttpConnectRequest for UnaryRequest<T> {
    fn uri(&self) -> &Uri {
        self.0.uri()
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

impl<T> From<http::Request<T>> for UnaryRequest<T> {
    fn from(req: http::Request<T>) -> Self {
        Self(req)
    }
}

impl<T> From<UnaryRequest<T>> for http::Request<T> {
    fn from(req: UnaryRequest<T>) -> Self {
        req.0
    }
}

/// A Connect streaming request.
pub struct StreamingRequest<T>(http::Request<T>);

impl<T> HttpConnectRequest for StreamingRequest<T> {
    fn uri(&self) -> &Uri {
        self.0.uri()
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

    fn accept_encoding(&self) -> impl Iterator<Item = &str> {
        self.headers()
            .get_all(CONNECT_ACCEPT_ENCODING)
            .into_iter()
            .filter_map(|val| val.to_str().ok())
    }
}

impl<T> From<http::Request<T>> for StreamingRequest<T> {
    fn from(req: http::Request<T>) -> Self {
        Self(req)
    }
}

impl<T> From<StreamingRequest<T>> for http::Request<T> {
    fn from(req: StreamingRequest<T>) -> Self {
        req.0
    }
}

/// A Connect unary GET request.
pub struct UnaryGetRequest {
    inner: http::Request<()>,
    query: HashMap<String, String>,
}

impl UnaryGetRequest {
    pub fn message(&self) -> Result<Cow<[u8]>, Error> {
        let message = self
            .query
            .get("message")
            .ok_or(Error::invalid_request("missing message"))?;
        let is_b64 = self.query.get("base64").map(|s| s.as_str()) == Some("1");
        if is_b64 {
            Ok(BASE64_URL_SAFE.decode(message)?.into())
        } else {
            Ok(
                match percent_encoding::percent_decode_str(message)
                    .decode_utf8()
                    .map_err(|_| Error::invalid_request("message not valid utf8"))?
                {
                    Cow::Borrowed(s) => s.as_bytes().into(),
                    Cow::Owned(s) => s.into_bytes().into(),
                },
            )
        }
    }
}

impl HttpConnectRequest for UnaryGetRequest {
    fn uri(&self) -> &Uri {
        self.inner.uri()
    }

    fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    fn message_codec(&self) -> Result<&str, Error> {
        self.query
            .get("encoding")
            .map(|s| s.as_str())
            .ok_or(Error::invalid_request("missing 'message' param"))
    }

    fn connect_protocol_version(&self) -> Option<&str> {
        self.query.get("connect")?.strip_prefix("v")
    }

    fn content_encoding(&self) -> Option<&str> {
        self.query.get("encoding").map(|s| s.as_str())
    }
}

impl From<http::Request<()>> for UnaryGetRequest {
    fn from(req: http::Request<()>) -> Self {
        let query: HashMap<_, _> =
            form_urlencoded::parse(req.uri().query().unwrap_or_default().as_bytes())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        Self { inner: req, query }
    }
}

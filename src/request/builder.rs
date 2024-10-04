use http::{
    header,
    uri::{Authority, Parts, PathAndQuery, Scheme},
    HeaderMap, HeaderName, HeaderValue, Method, Request, Uri,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE, Engine};

use crate::{
    common::{
        is_valid_http_token, CONNECT_ACCEPT_ENCODING, CONNECT_CONTENT_ENCODING,
        CONNECT_PROTOCOL_VERSION, CONNECT_TIMEOUT_MS, CONTENT_TYPE_PREFIX, PROTOCOL_VERSION_1,
    },
    metadata::Metadata,
    Error,
};

use super::{StreamingRequest, UnaryGetRequest, UnaryRequest};

#[derive(Debug, Default)]
pub struct RequestBuilder {
    scheme: Option<Scheme>,
    authority: Option<Authority>,
    path: Option<String>,
    metadata: HeaderMap,
    message_codec: Option<String>,
    timeout_ms: Option<HeaderValue>,
    content_encoding: Option<String>,
    accept_encoding: Vec<HeaderValue>,
}

impl RequestBuilder {
    /// Sets the URI scheme for this request.
    ///
    /// Defaults to [`Scheme::HTTPS`].
    pub fn scheme(
        mut self,
        scheme: impl TryInto<Scheme, Error: Into<Error>>,
    ) -> Result<Self, Error> {
        self.scheme = Some(scheme.try_into().map_err(Into::into)?);
        Ok(self)
    }

    /// Sets the authority (e.g. hostname) for this request.
    pub fn authority(
        mut self,
        authority: impl TryInto<Authority, Error: Into<Error>>,
    ) -> Result<Self, Error> {
        self.authority = Some(authority.try_into().map_err(Into::into)?);
        Ok(self)
    }

    /// Sets the path for this request.
    ///
    /// May not contain query params (i.e. the character '?').
    ///
    /// See also [`Self::protobuf_rpc`].
    pub fn path(mut self, path: impl Into<String>) -> Result<Self, Error> {
        let mut path = path.into();
        if path.contains('?') {
            return Err(Error::invalid_request(
                "path may not contain query params ('?')",
            ));
        }
        if !path.starts_with('/') {
            path = format!("/{path}");
        }
        self.path = Some(path);
        Ok(self)
    }

    /// Sets the path for this request from a protobuf RPC service/method.
    ///
    /// See also [`Self::protobuf_rpc_with_routing_prefix`].
    pub fn protobuf_rpc(
        self,
        full_service_name: impl AsRef<str>,
        method_name: impl AsRef<str>,
    ) -> Result<Self, Error> {
        self.path(format!(
            "/{}/{}",
            full_service_name.as_ref(),
            method_name.as_ref()
        ))
    }

    /// Sets the path for this request from a routing prefix and protobuf RPC
    /// service/method.
    pub fn protobuf_rpc_with_routing_prefix(
        self,
        routing_prefix: impl Into<String>,
        full_service_name: impl AsRef<str>,
        method_name: impl AsRef<str>,
    ) -> Result<Self, Error> {
        let mut routing_prefix = routing_prefix.into();
        if !routing_prefix.ends_with('/') {
            routing_prefix = format!("{routing_prefix}/");
        }
        self.path(format!(
            "{routing_prefix}{}/{}",
            full_service_name.as_ref(),
            method_name.as_ref()
        ))
    }

    /// Sets the scheme, authority, and path for this request from a URI.
    ///
    /// Any query part of the URI is discarded.
    pub fn uri(mut self, uri: impl TryInto<Uri, Error: Into<Error>>) -> Result<Self, Error> {
        let uri: Uri = uri.try_into().map_err(Into::into)?;
        let Parts {
            scheme,
            authority,
            path_and_query,
            ..
        } = uri.into_parts();
        self.scheme = scheme;
        self.authority = authority;
        self.path = path_and_query.map(|paq| paq.path().to_string());
        Ok(self)
    }

    /// Appends ASCII metadata to the request.
    pub fn ascii_metadata(
        mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl Into<String>,
    ) -> Result<Self, Error> {
        self.metadata.append_ascii(key, val)?;
        Ok(self)
    }

    /// Appends binary metadata to the request.
    pub fn binary_metadata(
        mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl AsRef<[u8]>,
    ) -> Result<Self, Error> {
        self.metadata.append_binary(key, val)?;
        Ok(self)
    }

    /// Sets the message codec for this request.
    ///
    /// Typical codecs are 'json' and 'proto', corresponding to the
    /// `content-type`s `application/json` and `application/proto`.
    ///
    /// The caller is responsible for making sure the request payload matches
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

    /// Sets the request timeout in milliseconds.
    pub fn timeout_ms(mut self, timeout_ms: u64) -> Result<Self, Error> {
        // Timeout-Milliseconds → {positive integer as ASCII string of at most 10 digits}
        let timeout = timeout_ms.to_string();
        if timeout.len() > 10 {
            return Err(Error::invalid_request("timeout too large"));
        }
        self.timeout_ms = Some(timeout.try_into().unwrap());
        Ok(self)
    }

    /// Clears the request timeout.
    pub fn clear_timeout(mut self) -> Self {
        self.timeout_ms = None;
        self
    }

    /// Sets the request content encoding (e.g. compression).
    pub fn content_encoding(mut self, content_encoding: impl Into<String>) -> Result<Self, Error> {
        let content_encoding = content_encoding.into();
        if !is_valid_http_token(&content_encoding) {
            return Err(Error::invalid_request("invalid content encoding"));
        }
        self.content_encoding = Some(content_encoding);
        Ok(self)
    }

    /// Sets the request accept encoding(s).
    pub fn accept_encoding<T: TryInto<HeaderValue, Error: Into<Error>>>(
        mut self,
        accept_encodings: impl IntoIterator<Item = T>,
    ) -> Result<Self, Error> {
        self.accept_encoding = accept_encodings
            .into_iter()
            .map(|v| v.try_into().map_err(Into::into))
            .collect::<Result<_, _>>()?;
        Ok(self)
    }

    /// Build logic common to all requests.
    fn common_request<T>(&mut self, method: Method, body: T) -> Result<http::Request<T>, Error> {
        let mut req = Request::new(body);
        *req.method_mut() = method;
        let mut headers: HeaderMap = std::mem::take(&mut self.metadata);
        // Connect-Protocol-Version → "connect-protocol-version" "1"
        headers.insert(CONNECT_PROTOCOL_VERSION, PROTOCOL_VERSION_1);
        // Timeout → "connect-timeout-ms" Timeout-Milliseconds
        if let Some(timeout) = self.timeout_ms.take() {
            headers.insert(CONNECT_TIMEOUT_MS, timeout);
        }
        *req.headers_mut() = headers;
        Ok(req)
    }

    /// Builds a [`UnaryRequest`].
    ///
    /// See: https://connectrpc.com/docs/protocol/#unary-request
    pub fn unary<T>(mut self, body: T) -> Result<UnaryRequest<T>, Error> {
        let mut req = self.common_request(Method::POST, body)?;
        *req.uri_mut() = build_uri(self.scheme, self.authority, self.path)?;

        // Unary-Content-Type → "content-type" "application/" Message-Codec
        if let Some(message_codec) = &self.message_codec {
            req.headers_mut().insert(
                header::CONTENT_TYPE,
                (format!("{CONTENT_TYPE_PREFIX}{message_codec}")).try_into()?,
            );
        }
        // Content-Encoding → "content-encoding" Content-Coding
        if let Some(content_encoding) = self.content_encoding.take() {
            req.headers_mut()
                .insert(header::CONTENT_ENCODING, content_encoding.try_into()?);
        }
        // Accept-Encoding → "accept-encoding" Content-Coding [...]
        for value in std::mem::take(&mut self.accept_encoding) {
            req.headers_mut().append(header::ACCEPT_ENCODING, value);
        }
        Ok(req.into())
    }

    /// Builds a [`StreamingRequest`].
    ///
    /// https://connectrpc.com/docs/protocol/#streaming-request
    pub fn streaming<T>(mut self, body: T) -> Result<StreamingRequest<T>, Error> {
        let mut req = self.common_request(Method::POST, body)?;
        *req.uri_mut() = build_uri(self.scheme, self.authority, self.path)?;

        // Streaming-Content-Type → "content-type" "application/connect+" [...]
        if let Some(message_codec) = &self.message_codec {
            req.headers_mut().insert(
                header::CONTENT_TYPE,
                (format!("{CONTENT_TYPE_PREFIX}{message_codec}")).try_into()?,
            );
        }
        // Streaming-Content-Encoding → "connect-content-encoding" Content-Coding
        if let Some(content_encoding) = self.content_encoding.take() {
            req.headers_mut()
                .insert(CONNECT_CONTENT_ENCODING, content_encoding.try_into()?);
        }
        // Streaming-Accept-Encoding → "connect-accept-encoding" Content-Coding [...]
        for value in std::mem::take(&mut self.accept_encoding) {
            req.headers_mut().append(CONNECT_ACCEPT_ENCODING, value);
        }
        Ok(req.into())
    }

    /// Builds a [`UnaryGetRequest`].
    ///
    // https://connectrpc.com/docs/protocol/#unary-get-request
    pub fn unary_get(mut self, message: impl AsRef<[u8]>) -> Result<UnaryGetRequest, Error> {
        let mut req = self.common_request(Method::GET, ())?;
        *req.method_mut() = Method::GET;

        let path_and_query = {
            let path = self.path.ok_or(Error::invalid_request("path required"))?;
            let query = {
                let mut query = form_urlencoded::Serializer::new("?".to_string());
                query
                    // Message-Query → "message=" (*{percent-encoded octet})
                    .append_pair("message", &BASE64_URL_SAFE.encode(message))
                    // Base64-Query → "&base64=1"
                    .append_pair("base64", "1")
                    // Connect-Version-Query → "&connect=v1"
                    .append_pair("connect", "v1");
                if let Some(message_codec) = &self.message_codec {
                    // Encoding-Query → "&encoding=" Message-Codec
                    query.append_pair("encoding", message_codec);
                } else {
                    return Err(Error::invalid_request("message codec required"));
                }
                if let Some(content_encoding) = &self.content_encoding {
                    // Compression-Query → "&compression=" Content-Coding
                    query.append_pair("compression", content_encoding);
                }
                query.finish()
            };
            Some(format!("{path}?{query}"))
        };
        *req.uri_mut() = build_uri(self.scheme, self.authority, path_and_query)?;

        // Accept-Encoding (same as unary)
        for value in std::mem::take(&mut self.accept_encoding) {
            req.headers_mut().append(header::ACCEPT_ENCODING, value);
        }
        Ok(req.into())
    }
}

fn build_uri(
    scheme: Option<Scheme>,
    authority: Option<Authority>,
    path_and_query: Option<impl TryInto<PathAndQuery, Error: Into<Error>>>,
) -> Result<Uri, Error> {
    Ok(Uri::from_parts({
        let mut parts = Parts::default();
        parts.scheme = scheme;
        parts.authority = authority;
        parts.path_and_query = path_and_query
            .map(TryInto::try_into)
            .transpose()
            .map_err(Into::into)?;
        parts
    })?)
}

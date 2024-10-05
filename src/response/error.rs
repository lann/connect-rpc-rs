use http::{header, HeaderMap, HeaderValue};

use crate::{common::base64_decode, metadata::Metadata, Error};

const ERROR_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("application/json");

/// A Connect error.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConnectError {
    #[serde(default, deserialize_with = "deserialize_error_code")]
    code: Option<ConnectCode>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ConnectErrorDetail>,
    #[serde(skip)]
    headers: HeaderMap,
}

impl ConnectError {
    pub fn new(code: ConnectCode, message: impl std::fmt::Display) -> Self {
        Self {
            code: Some(code),
            message: message.to_string(),
            details: Default::default(),
            headers: Default::default(),
        }
    }

    pub fn code(&self) -> ConnectCode {
        self.code.unwrap_or(ConnectCode::Unknown)
    }

    pub fn metadata(&self) -> &impl Metadata {
        &self.headers
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(serde_json::to_value(self.code()).unwrap().as_str().unwrap())?;
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

impl<T: AsRef<[u8]>> From<http::Response<T>> for ConnectError {
    fn from(resp: http::Response<T>) -> Self {
        let (parts, body) = resp.into_parts();
        let error = if parts.headers.get(header::CONTENT_TYPE) == Some(&ERROR_CONTENT_TYPE) {
            match serde_json::from_slice::<ConnectError>(body.as_ref()) {
                Ok(mut error) => {
                    error.code.get_or_insert_with(|| parts.status.into());
                    Some(error)
                }
                Err(err) => {
                    tracing::debug!(?err, "Failed to decode error JSON");
                    None
                }
            }
        } else {
            None
        };
        let mut error = error.unwrap_or_else(|| Self::new(parts.status.into(), "request invalid"));
        error.headers = parts.headers;
        error
    }
}

impl From<Error> for ConnectError {
    fn from(err: Error) -> Self {
        let code = match err {
            Error::ConnectError(connect_error) => return connect_error,
            Error::InvalidResponse(_)
            | Error::UnacceptableEncoding(_)
            | Error::UnexpectedMessageCodec(_) => ConnectCode::Internal,
            _ => ConnectCode::Unknown,
        };
        let message = match &err {
            Error::UnacceptableEncoding(_) | Error::UnexpectedMessageCodec(_) => err.to_string(),
            _ => "".into(),
        };
        Self::new(code, message)
    }
}

fn deserialize_error_code<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ConnectCode>, D::Error> {
    use serde::Deserialize;
    Option::<ConnectCode>::deserialize(deserializer).or(Ok(None))
}

/// ConnectCode represents categories of errors as codes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectCode {
    /// The operation completed successfully.
    Ok,
    /// The operation was cancelled.
    Canceled,
    /// Unknown error.
    Unknown,
    /// Client specified an invalid argument.
    InvalidArgument,
    /// Deadline expired before operation could complete.
    DeadlineExceeded,
    /// Some requested entity was not found.
    NotFound,
    /// Some entity that we attempted to create already exists.
    AlreadyExists,
    /// The caller does not have permission to execute the specified operation.
    PermissionDenied,
    /// Some resource has been exhausted.
    ResourceExhausted,
    /// The system is not in a state required for the operation's execution.
    FailedPrecondition,
    /// The operation was aborted.
    Aborted,
    /// Operation was attempted past the valid range.
    OutOfRange,
    /// Operation is not implemented or not supported.
    Unimplemented,
    /// Internal error.
    Internal,
    /// The service is currently unavailable.
    Unavailable,
    /// Unrecoverable data loss or corruption.
    DataLoss,
    /// The request does not have valid authentication credentials
    Unauthenticated,
}

// https://connectrpc.com/docs/protocol/#http-to-error-code
impl From<http::StatusCode> for ConnectCode {
    fn from(code: http::StatusCode) -> Self {
        use http::StatusCode;
        match code {
            StatusCode::BAD_REQUEST => Self::Internal,
            StatusCode::UNAUTHORIZED => Self::Unauthenticated,
            StatusCode::FORBIDDEN => Self::PermissionDenied,
            StatusCode::NOT_FOUND => Self::Unimplemented,
            StatusCode::NOT_IMPLEMENTED => Self::Unimplemented,
            StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT => Self::Unavailable,
            _ => Self::Unknown,
        }
    }
}

/// Connect error detail.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConnectErrorDetail {
    #[serde(rename = "type")]
    pub proto_type: String,
    #[serde(rename = "value")]
    pub value_base64: String,
}

impl ConnectErrorDetail {
    pub fn type_url(&self) -> String {
        format!("type.googleapis.com/{}", self.proto_type)
    }

    pub fn value(&self) -> Result<Vec<u8>, Error> {
        base64_decode(&self.value_base64)
    }
}

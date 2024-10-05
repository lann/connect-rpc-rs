use std::future::Future;

use bytes::Bytes;

use crate::{
    request::{ConnectRequest, UnaryGetRequest, UnaryRequest},
    response::{
        error::{ConnectCode, ConnectError},
        UnaryResponse, ValidateOpts,
    },
    Error,
};

pub trait ReqwestClientExt {
    /// Executes a Connect RPC [`UnaryRequest`].
    fn execute_unary(
        &self,
        req: UnaryRequest<impl Into<reqwest::Body>>,
    ) -> impl Future<Output = Result<UnaryResponse<Bytes>, Error>>;

    /// Executes a Connect RPC [`UnaryGetRequest`].
    fn execute_unary_get(
        &self,
        req: UnaryGetRequest,
    ) -> impl Future<Output = Result<UnaryResponse<Bytes>, Error>>;
}

impl ReqwestClientExt for reqwest::Client {
    async fn execute_unary(
        &self,
        req: UnaryRequest<impl Into<reqwest::Body>>,
    ) -> Result<UnaryResponse<Bytes>, Error> {
        let validate_opts = ValidateOpts::from_request(&req);
        let resp = self.execute(req.try_into()?).await?;
        let connect_resp: UnaryResponse<_> = response_to_http_bytes(resp).await?.into();
        connect_resp.result(&validate_opts)
    }

    async fn execute_unary_get(&self, req: UnaryGetRequest) -> Result<UnaryResponse<Bytes>, Error> {
        let validate_opts = ValidateOpts::from_request(&req);
        let resp = self.execute(req.try_into()?).await?;
        let connect_resp: UnaryResponse<_> = response_to_http_bytes(resp).await?.into();
        connect_resp.result(&validate_opts)
    }
}

async fn response_to_http_bytes(
    mut resp: reqwest::Response,
) -> Result<http::Response<Bytes>, Error> {
    let status = resp.status();
    let headers = std::mem::take(resp.headers_mut());
    let body = resp.bytes().await?;
    let mut http_resp = http::Response::new(body);
    *http_resp.status_mut() = status;
    *http_resp.headers_mut() = headers;
    Ok(http_resp)
}

impl<T: Into<reqwest::Body>> TryFrom<UnaryRequest<T>> for reqwest::Request {
    type Error = Error;

    fn try_from(req: UnaryRequest<T>) -> Result<Self, Self::Error> {
        let timeout = req.timeout();
        let mut req = reqwest::Request::try_from(http::Request::from(req))?;
        *req.timeout_mut() = timeout;
        Ok(req)
    }
}

impl TryFrom<UnaryGetRequest> for reqwest::Request {
    type Error = Error;

    fn try_from(req: UnaryGetRequest) -> Result<Self, Self::Error> {
        let timeout = req.timeout();
        let http_req = http::Request::from(req).map(|()| reqwest::Body::default());
        let mut req = reqwest::Request::try_from(http_req)?;
        *req.timeout_mut() = timeout;
        Ok(req)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::ConnectError(ConnectError::new(
                ConnectCode::DeadlineExceeded,
                "request timed out",
            ))
        } else {
            Self::ReqwestError(err)
        }
    }
}

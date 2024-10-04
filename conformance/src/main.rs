use std::{
    collections::HashMap,
    io::{ErrorKind, Write},
    time::Duration,
};

use anyhow::{bail, ensure};
use connect_rpc::{
    metadata::Metadata,
    request::builder::RequestBuilder,
    response::{error::ConnectCode, ConnectResponse, UnaryResponse, ValidateOpts},
};
use http::HeaderMap;
use prost::Message;
use tokio::{io::AsyncReadExt, task::JoinSet};
use tracing_subscriber::{fmt::format, prelude::*, EnvFilter};

mod proto {
    include!("../gen/connectrpc.conformance.v1.rs");
}
use proto::{
    client_compat_response::Result as ClientCompatResult, ClientCompatRequest,
    ClientCompatResponse, ClientErrorResult, ClientResponseResult, Error as ResponseError, Header,
    HttpVersion,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .event_format(format().compact()),
        )
        .with(EnvFilter::from_default_env())
        .init();

    let mut tasks = JoinSet::new();
    while let Some(req) = read_request().await? {
        tasks.spawn(handle_client_test(req));
        // TODO configure parallelism
        while tasks.len() > 16 {
            tasks.join_next().await;
        }
    }
    tasks.join_all().await;
    Ok(())
}

async fn handle_client_test(test: ClientCompatRequest) {
    let test_name = test.test_name.clone();
    tracing::debug!(test_name, "Running client test");

    let result = match run_client_test(test).await {
        Ok(response) => {
            tracing::debug!(?response, "Sending response");
            ClientCompatResult::Response(response)
        }
        Err(err) => ClientCompatResult::Error(ClientErrorResult {
            message: err.to_string(),
        }),
    };
    if let Err(err) = write_response(ClientCompatResponse {
        test_name,
        result: Some(result),
    }) {
        panic!("Error writing response: {err:?}");
    }
}

async fn run_client_test(test: ClientCompatRequest) -> anyhow::Result<ClientResponseResult> {
    tracing::trace!(?test);

    // Assert supported test features
    ensure!(test.protocol() == proto::Protocol::Connect);
    ensure!(test.codec() == proto::Codec::Proto);
    ensure!(test.compression() == proto::Compression::Identity);
    ensure!(test.server_tls_cert.is_empty());
    ensure!(test.client_tls_creds.is_none());

    let client = {
        let builder = reqwest::Client::builder();
        let builder = match test.http_version() {
            HttpVersion::Unspecified => builder,
            HttpVersion::HttpVersion1 => builder.http1_only(),
            HttpVersion::HttpVersion2 => builder.http2_prior_knowledge(),
            HttpVersion::HttpVersion3 => bail!("HTTP3 not supported"),
        };
        builder.build()?
    };

    let validate_opts;
    let mut req: reqwest::Request = {
        let mut builder = RequestBuilder::default()
            .scheme("http")?
            .authority(format!("{}:{}", test.host, test.port))?
            .protobuf_rpc(test.service(), test.method())?
            .message_codec("proto")?;

        if let Some(timeout_ms) = test.timeout_ms {
            builder = builder.timeout_ms(timeout_ms.into())?;
        }

        for header in test.request_headers {
            for value in header.value {
                builder = builder.ascii_metadata(&header.name, value)?;
            }
        }

        let msg = &test.request_messages[0].value;
        tracing::trace!(msg = %msg.escape_ascii());
        let http_req: http::Request<Vec<u8>> = if test.use_get_http_method {
            let req = builder.unary_get(msg)?;
            validate_opts = ValidateOpts::from_request(&req);
            http::Request::from(req).map(|_| vec![])
        } else {
            let req = builder.unary(msg.clone())?;
            validate_opts = ValidateOpts::from_request(&req);
            req.into()
        };
        http_req.try_into()?
    };
    if let Some(timeout_ms) = test.timeout_ms {
        *req.timeout_mut() = Some(Duration::from_millis(timeout_ms.into()));
    }
    tracing::trace!(?req);

    let mut resp = match client.execute(req).await {
        Ok(resp) => resp,
        Err(err) => {
            if err.is_timeout() {
                return Ok(ConnectCode::DeadlineExceeded.into());
            }
            return Err(err.into());
        }
    };
    tracing::trace!(?resp);

    if test.cancel.is_some() {
        return Ok(ConnectCode::Canceled.into());
    }

    let (response_headers, response_trailers) = headers_and_trailers(resp.headers());

    let resp = {
        let (mut parts, ()) = http::Response::default().into_parts();
        parts.status = resp.status();
        parts.headers = std::mem::take(resp.headers_mut());
        let body = resp.bytes().await?;
        UnaryResponse::from(http::Response::from_parts(parts, body))
    };
    if !resp.status().is_success() {
        tracing::trace!(body = %resp.body().as_ref().escape_ascii());
    }

    let (payloads, error) = match resp.error(&validate_opts) {
        None => {
            let resp_msg = proto::UnaryResponse::decode(resp.body().as_ref())?;
            (vec![resp_msg.payload.unwrap_or_default()], None)
        }
        Some(connect_error) => {
            let code = connect_error.code();
            let details = connect_error
                .details
                .into_iter()
                .map(|detail| {
                    Ok(prost_types::Any {
                        type_url: detail.type_url(),
                        value: detail.value()?,
                    })
                })
                .collect::<anyhow::Result<_>>()?;
            (
                vec![],
                Some(ResponseError {
                    code: proto::Code::from(code) as i32,
                    message: Some(connect_error.message),
                    details,
                }),
            )
        }
    };

    Ok(ClientResponseResult {
        response_headers,
        response_trailers,
        payloads,
        error,
        ..Default::default()
    })
}

async fn read_request<T: Message + Default>() -> anyhow::Result<Option<T>> {
    let len = match tokio::io::stdin().read_u32().await {
        Ok(len) => len,
        Err(err) if err.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        err @ Err(_) => err?,
    };
    let mut buf = vec![0; len.try_into().unwrap()];
    tokio::io::stdin().read_exact(&mut buf).await?;
    let config = T::decode(&buf[..])?;
    Ok(Some(config))
}

fn write_response(resp: impl Message) -> anyhow::Result<()> {
    let buf = resp.encode_to_vec();
    let len: u32 = buf.len().try_into()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&len.to_be_bytes())?;
    stdout.write_all(&buf)?;
    stdout.flush()?;
    Ok(())
}

fn headers_and_trailers(metadata: &HeaderMap) -> (Vec<Header>, Vec<Header>) {
    let mut headers: HashMap<&str, Header> = HashMap::new();
    let mut trailers: HashMap<&str, Header> = HashMap::new();
    for (key, val) in metadata.iter_ascii() {
        let map = if key.ends_with("-trailer") {
            &mut trailers
        } else {
            &mut headers
        };
        map.entry(key)
            .or_insert_with(|| Header {
                name: key.to_string(),
                ..Default::default()
            })
            .value
            .push(val.to_string());
    }
    (
        headers.into_values().collect(),
        trailers.into_values().collect(),
    )
}

impl From<ConnectCode> for ClientResponseResult {
    fn from(code: ConnectCode) -> Self {
        Self {
            error: Some(ResponseError {
                code: proto::Code::from(code) as i32,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

impl From<ConnectCode> for proto::Code {
    fn from(code: ConnectCode) -> Self {
        match code {
            ConnectCode::Ok => Self::Unspecified,
            ConnectCode::Canceled => Self::Canceled,
            ConnectCode::Unknown => Self::Unknown,
            ConnectCode::InvalidArgument => Self::InvalidArgument,
            ConnectCode::DeadlineExceeded => Self::DeadlineExceeded,
            ConnectCode::NotFound => Self::NotFound,
            ConnectCode::AlreadyExists => Self::AlreadyExists,
            ConnectCode::PermissionDenied => Self::PermissionDenied,
            ConnectCode::ResourceExhausted => Self::ResourceExhausted,
            ConnectCode::FailedPrecondition => Self::FailedPrecondition,
            ConnectCode::Aborted => Self::Aborted,
            ConnectCode::OutOfRange => Self::OutOfRange,
            ConnectCode::Unimplemented => Self::Unimplemented,
            ConnectCode::Internal => Self::Internal,
            ConnectCode::Unavailable => Self::Unavailable,
            ConnectCode::DataLoss => Self::DataLoss,
            ConnectCode::Unauthenticated => Self::Unauthenticated,
        }
    }
}

//! Reverse proxy to a project's dev origin (Vite, Next, anything on `127.0.0.1:<port>`).
//!
//! Every path, method, header and streamed body is forwarded; HTML responses get the bridge injected; WebSocket
//! upgrades (Vite's `/@vite/client` HMR socket, plain `ws` endpoints) are tunnelled byte for byte. The request
//! asks for `identity` so HTML arrives decodable, but a `gzip`/`deflate` response is still handled.

use axum::body::{Body, to_bytes};
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::uri::{Authority, Scheme};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, Uri};
use axum::response::IntoResponse;
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};

use super::FrameError;
use super::inject;

/// Bodies above this are not buffered for injection (they are almost certainly not HTML).
const MAX_HTML: usize = 32 * 1024 * 1024;

type HttpClient = Client<HttpConnector, Body>;

/// A connection to one dev origin.
#[allow(
    missing_debug_implementations,
    reason = "the hyper client keeps no useful debug state"
)]
pub struct Proxy {
    client: HttpClient,
    scheme: Scheme,
    authority: Authority,
    host: HeaderValue,
}

impl Proxy {
    /// Parses an `http://host:port` origin. Plain HTTP only: dev servers here are local and unencrypted.
    ///
    /// # Errors
    /// A non-HTTP URL or an authority that does not parse.
    pub fn new(origin: &str) -> Result<Self, FrameError> {
        let trimmed = origin.trim().trim_end_matches('/');
        let Some(rest) = trimmed.strip_prefix("http://") else {
            return Err(FrameError::Config(format!(
                "origin must be an http:// URL (local dev servers are plain HTTP): {origin}"
            )));
        };
        let raw = rest.split('/').next().unwrap_or_default();
        let authority: Authority = raw
            .parse()
            .map_err(|error| FrameError::Config(format!("origin {origin}: {error}")))?;
        if authority.port_u16().is_none() {
            return Err(FrameError::Config(format!(
                "origin {origin} has no port; link the dev server's explicit port"
            )));
        }
        let host = HeaderValue::from_str(authority.as_str())
            .map_err(|error| FrameError::Config(format!("origin {origin}: {error}")))?;
        let client: HttpClient = Client::builder(TokioExecutor::new()).build_http();
        Ok(Self {
            client,
            scheme: Scheme::HTTP,
            authority,
            host,
        })
    }

    /// Forwards one request and adapts the response.
    pub async fn forward(&self, mut request: Request<Body>) -> Response<Body> {
        let upgraded = is_upgrade(request.headers()).then(|| hyper::upgrade::on(&mut request));
        let head = request.method() == Method::HEAD;
        let (parts, body) = request.into_parts();
        let path_and_query = parts
            .uri
            .path_and_query()
            .map_or("/", |value| value.as_str());
        let Ok(uri) = Uri::builder()
            .scheme(self.scheme.clone())
            .authority(self.authority.clone())
            .path_and_query(path_and_query)
            .build()
        else {
            return (StatusCode::BAD_REQUEST, "bad request path").into_response();
        };

        let mut upstream = Request::new(body);
        *upstream.method_mut() = parts.method;
        *upstream.uri_mut() = uri;
        *upstream.headers_mut() = self.headers(&parts.headers);

        match self.client.request(upstream).await {
            Ok(response) => self.respond(response, upgraded, head).await,
            Err(error) => {
                tracing::warn!(%error, origin = %self.authority, "dev server unreachable");
                (
                    StatusCode::BAD_GATEWAY,
                    format!("dev server {} unreachable: {error}", self.authority),
                )
                    .into_response()
            }
        }
    }

    /// Headers sent upstream: everything but hop-by-hop, minus our cookie, with identity encoding.
    fn headers(&self, input: &HeaderMap) -> HeaderMap {
        let upgrade = is_upgrade(input);
        let mut out = HeaderMap::with_capacity(input.len() + 2);
        for (name, value) in input {
            if name == header::HOST
                || name == header::ACCEPT_ENCODING
                || name == header::COOKIE
                || (is_hop_by_hop(name)
                    && !(upgrade && (name == header::CONNECTION || name == header::UPGRADE)))
            {
                continue;
            }
            out.append(name.clone(), value.clone());
        }
        out.insert(header::HOST, self.host.clone());
        // Ask for an unencoded body: the bridge goes into HTML, which we must be able to read.
        out.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("identity"),
        );
        if let Some(cookie) = input
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
        {
            let cookie = strip_penwise_cookie(cookie);
            if let Ok(value) = HeaderValue::from_str(&cookie) {
                out.insert(header::COOKIE, value);
            }
        }
        out
    }

    /// Turns the upstream response into ours: 101 tunnels, HTML injection, everything else streamed.
    async fn respond(
        &self,
        mut response: Response<Incoming>,
        upgraded: Option<hyper::upgrade::OnUpgrade>,
        head: bool,
    ) -> Response<Body> {
        if response.status() == StatusCode::SWITCHING_PROTOCOLS {
            let upstream_upgrade = hyper::upgrade::on(&mut response);
            let Some(client_upgrade) = upgraded else {
                tracing::warn!("dev server switched protocols but the client did not ask to");
                return (StatusCode::BAD_GATEWAY, "unexpected protocol switch").into_response();
            };
            tokio::spawn(async move {
                match tokio::try_join!(client_upgrade, upstream_upgrade) {
                    Ok((mine, theirs)) => {
                        let (mut mine, mut theirs) = (TokioIo::new(mine), TokioIo::new(theirs));
                        if let Err(error) =
                            tokio::io::copy_bidirectional(&mut mine, &mut theirs).await
                        {
                            tracing::debug!(%error, "upgraded connection closed");
                        }
                    }
                    Err(error) => tracing::debug!(%error, "protocol upgrade failed"),
                }
            });
            let mut out = Response::new(Body::empty());
            *out.status_mut() = response.status();
            *out.headers_mut() = response.headers().clone();
            return out;
        }

        let status = response.status();
        let headers = response.headers().clone();
        let content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let encoding = headers
            .get(header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok());
        let (parts, body) = response.into_parts();
        let mut out_headers = without_hop_by_hop(&headers);

        let bodyless = matches!(status, StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED);
        if inject::is_html(content_type) && !head && !bodyless {
            let bytes = match to_bytes(Body::new(body), MAX_HTML).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::warn!(%error, origin = %self.authority, "could not buffer HTML");
                    return (StatusCode::BAD_GATEWAY, "upstream HTML could not be read")
                        .into_response();
                }
            };
            if let Some(decoded) = inject::decode(encoding, &bytes) {
                let injected = inject::inject(&decoded);
                out_headers.remove(header::CONTENT_ENCODING);
                out_headers.remove(header::TRANSFER_ENCODING);
                if let Ok(length) = HeaderValue::from_str(&injected.len().to_string()) {
                    out_headers.insert(header::CONTENT_LENGTH, length);
                }
                let mut out = Response::new(Body::from(injected));
                *out.status_mut() = status;
                *out.headers_mut() = out_headers;
                return out;
            }
            // An encoding we cannot decode (brotli, zstd): hand it over untouched rather than corrupt it.
            let mut out = Response::new(Body::from(bytes));
            *out.status_mut() = status;
            *out.headers_mut() = out_headers;
            return out;
        }

        let mut out = Response::new(Body::new(body));
        *out.status_mut() = status;
        *out.headers_mut() = out_headers;
        *out.version_mut() = parts.version;
        out
    }
}

/// `true` when the client asked for a protocol switch (WebSocket).
fn is_upgrade(headers: &HeaderMap) -> bool {
    if headers.contains_key(header::UPGRADE) {
        return true;
    }
    headers
        .get(header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        })
}

/// Hop-by-hop headers (RFC 9110 §7.6.1) are never forwarded, except the upgrade pair on an upgrade request.
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn without_hop_by_hop(headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        if !is_hop_by_hop(name) {
            out.append(name.clone(), value.clone());
        }
    }
    out
}

/// Drops our own cookie from a `Cookie` header so the dev server never sees the access token.
fn strip_penwise_cookie(cookie: &str) -> String {
    cookie
        .split(';')
        .map(str::trim)
        .filter(|pair| {
            let name = pair.split_once('=').map_or(*pair, |(name, _)| name).trim();
            !name.eq_ignore_ascii_case(super::COOKIE)
        })
        .filter(|pair| !pair.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

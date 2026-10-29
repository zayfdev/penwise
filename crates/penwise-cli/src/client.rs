//! The loopback JSON client for the project registry (contract C2).
//!
//! The token comes from `GET /pair/local` — the same loopback-only route the desktop window uses, so nobody
//! copies a secret around. Every registry route then takes it as `X-Penwise-Token`.
//!
//! Requests carry no proxy configuration: a `HTTP_PROXY` in the environment must not redirect a loopback call.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Where the server listens unless `PENWISE_URL` says otherwise.
const DEFAULT_PORT: u16 = 7420;
/// Registry calls are local; anything slower than this is a sick server, not a slow one.
const API_TIMEOUT: Duration = Duration::from_secs(5);
/// A dev origin probe: a refused connection is immediate, a hung one must not hold the status output.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const TOKEN_HEADER: &str = "X-Penwise-Token";
const PAIR_PATH: &str = "/pair/local";
const PROJECTS_PATH: &str = "/projects";

/// `PENWISE_PORT`, else 7420.
#[must_use]
pub fn port() -> u16 {
    std::env::var("PENWISE_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// `PENWISE_URL`, else `http://127.0.0.1:<port>`; never with a trailing slash.
#[must_use]
pub fn base_url() -> String {
    match std::env::var("PENWISE_URL") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_owned(),
        _ => format!("http://127.0.0.1:{}", port()),
    }
}

/// One entry of `GET /projects` (fields the CLI shows; anything else in the answer is ignored).
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub root: String,
    /// `"static"` or `"proxy"`.
    #[serde(default)]
    pub kind: String,
    /// The folder Penwise serves, for a static project.
    #[serde(default, rename = "static")]
    pub static_dir: Option<String>,
    /// The dev server origin, for a proxied project.
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub port: u16,
}

/// The body of `POST /projects`; exactly one of `static`/`origin` is set.
#[derive(Debug, Serialize)]
pub struct Register<'a> {
    pub name: &'a str,
    pub root: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#static: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<&'a str>,
}

/// A paired client for one server.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}

impl Client {
    /// Pairs with the server on `base` and keeps its token.
    ///
    /// # Errors
    /// The server does not answer, or refuses `/pair/local` (not a loopback URL).
    pub fn connect(base: &str) -> Result<Self, Error> {
        let base = base.trim_end_matches('/').to_owned();
        let http = http(API_TIMEOUT)?;
        let response = http
            .get(format!("{base}{PAIR_PATH}"))
            .send()
            .map_err(|error| unreachable(&base, &error))?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(Error::Refused {
                request: "GET /pair/local".to_owned(),
                base,
                status: 403,
                detail: "pairing is loopback-only; use PENWISE_URL=http://127.0.0.1:<port>"
                    .to_owned(),
            });
        }
        let response = status_ok(response, "GET /pair/local", &base)?;
        let token: Token = response.json().map_err(|error| Error::Refused {
            request: "GET /pair/local".to_owned(),
            base: base.clone(),
            status: 200,
            detail: format!("no token in the answer: {error}"),
        })?;
        Ok(Self {
            http,
            base,
            token: token.token,
        })
    }

    /// `GET /projects`.
    ///
    /// # Errors
    /// The server does not answer, or refuses the request.
    pub fn projects(&self) -> Result<Vec<Project>, Error> {
        let response = status_ok(
            self.get(PROJECTS_PATH)
                .send()
                .map_err(|error| self.unreachable(&error))?,
            "GET /projects",
            &self.base,
        )?;
        response.json().map_err(|error| Error::Refused {
            request: "GET /projects".to_owned(),
            base: self.base.clone(),
            status: 200,
            detail: format!("unexpected JSON: {error}"),
        })
    }

    /// `POST /projects` — idempotent by `root` on the server, so linking twice is safe.
    ///
    /// # Errors
    /// The server does not answer, or the registration is rejected.
    pub fn register(&self, body: &Register<'_>) -> Result<Project, Error> {
        let response = status_ok(
            self.post(PROJECTS_PATH)
                .json(body)
                .send()
                .map_err(|error| self.unreachable(&error))?,
            "POST /projects",
            &self.base,
        )?;
        response.json().map_err(|error| Error::Refused {
            request: "POST /projects".to_owned(),
            base: self.base.clone(),
            status: 200,
            detail: format!("unexpected JSON: {error}"),
        })
    }

    /// `DELETE /projects/:id`. An unknown id is success: the goal (no entry) is reached.
    ///
    /// # Errors
    /// The server does not answer, or deletes nothing.
    pub fn delete(&self, id: &str) -> Result<bool, Error> {
        let response = self
            .http
            .delete(format!("{}{PROJECTS_PATH}/{id}", self.base))
            .header(TOKEN_HEADER, &self.token)
            .send()
            .map_err(|error| self.unreachable(&error))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        status_ok(response, "DELETE /projects/:id", &self.base)?;
        Ok(true)
    }

    fn get(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.base))
            .header(TOKEN_HEADER, &self.token)
    }

    fn post(&self, path: &str) -> reqwest::blocking::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.base))
            .header(TOKEN_HEADER, &self.token)
    }

    fn unreachable(&self, error: &reqwest::Error) -> Error {
        unreachable(&self.base, error)
    }
}

/// Whether `url` answers at all. Any 2xx counts: a dev server that answers its root is up.
#[must_use]
pub fn answers(url: &str) -> bool {
    http(PROBE_TIMEOUT).is_ok_and(|http| {
        http.get(url)
            .send()
            .is_ok_and(|response| response.status().is_success())
    })
}

fn http(timeout: Duration) -> Result<reqwest::blocking::Client, Error> {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .no_proxy()
        .build()
        .map_err(|error| Error::Client(error.to_string()))
}

fn unreachable(base: &str, error: &reqwest::Error) -> Error {
    Error::Unreachable {
        base: base.to_owned(),
        detail: error.to_string(),
    }
}

fn status_ok(
    response: reqwest::blocking::Response,
    request: &str,
    base: &str,
) -> Result<reqwest::blocking::Response, Error> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    let detail = if body.trim().is_empty() {
        status.canonical_reason().unwrap_or("no message").to_owned()
    } else {
        body.trim().to_owned()
    };
    Err(Error::Refused {
        request: request.to_owned(),
        base: base.to_owned(),
        status: status.as_u16(),
        detail,
    })
}

#[derive(Deserialize)]
struct Token {
    token: String,
}

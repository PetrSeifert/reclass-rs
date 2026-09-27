//! Authentication happens before WebSocket upgrade or RPC body extraction.
use std::{
    net::SocketAddr,
    sync::Arc,
};

use axum::{
    extract::{
        Request,
        State,
    },
    http::{
        header,
        HeaderMap,
        StatusCode,
        Uri,
    },
    middleware::Next,
    response::Response,
};
use rand::TryRngCore;
use subtle::ConstantTimeEq;

pub struct Access {
    token: String,
    origins: Vec<String>,
}

impl Access {
    pub fn new(token: String, bind: SocketAddr, mut origins: Vec<String>) -> Self {
        // Never derive trust from the request's Host or forwarded headers.
        if !bind.ip().is_unspecified() {
            origins.push(format!("http://{bind}"));
        }
        if bind.ip().is_loopback() {
            origins.push(format!("http://localhost:{}", bind.port()));
        }
        Self { token, origins }
    }

    fn matches(&self, candidate: &str) -> bool {
        bool::from(self.token.as_bytes().ct_eq(candidate.as_bytes()))
    }
}

pub fn parse_origin(value: &str) -> Result<String, String> {
    let uri: Uri = value.parse().map_err(|_| "expected an HTTP(S) origin")?;
    if !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.authority().is_none()
        || value.contains(['@', '*', '?', '#'])
        || value.trim_end_matches('/').len() != value.len()
        || uri.path() != "/" && !uri.path().is_empty()
    {
        return Err("expected an exact HTTP(S) origin without a path or trailing slash".into());
    }
    Ok(value.to_owned())
}

pub fn startup_token() -> anyhow::Result<String> {
    match std::env::var("RECLASS_API_TOKEN") {
        Ok(token) => {
            anyhow::ensure!(
                token.len() >= 64 && token.bytes().all(|b| b.is_ascii_hexdigit()),
                "RECLASS_API_TOKEN must contain at least 64 hexadecimal characters"
            );
            Ok(token)
        }
        Err(std::env::VarError::NotPresent) => {
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng
                .try_fill_bytes(&mut bytes)
                .map_err(|_| anyhow::anyhow!("could not generate API token"))?;
            let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            // Deliberate local bootstrap output, never exposed by HTTP or static assets.
            eprintln!("API token for this run (paste into the web UI): {token}");
            Ok(token)
        }
        Err(_) => anyhow::bail!("RECLASS_API_TOKEN is not valid Unicode"),
    }
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, StatusCode> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    first
        .map(|v| v.to_str().map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()
}

pub async fn authorize(
    State(access): State<Arc<Access>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(origin) = single_header(request.headers(), "origin")? {
        if !access.origins.iter().any(|allowed| allowed == origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    let authorized = match single_header(request.headers(), header::AUTHORIZATION.as_str())? {
        Some(value) => value
            .strip_prefix("Bearer ")
            .is_some_and(|token| access.matches(token)),
        None if request.uri().path() == "/ws" => {
            // Browser WebSocket cannot set Authorization. Carry the token in the
            // offered protocols, but select only "reclass", never echo the secret.
            single_header(request.headers(), "sec-websocket-protocol")?.is_some_and(|value| {
                value.split(',').map(str::trim).any(|protocol| {
                    protocol
                        .strip_prefix("reclass-token.")
                        .is_some_and(|token| access.matches(token))
                })
            })
        }
        None => false,
    };
    if !authorized {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

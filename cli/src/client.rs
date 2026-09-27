//! Calls `POST /api/rpc` on a running reclass-server.

use anyhow::{
    anyhow,
    bail,
    Result,
};
use serde_json::{
    json,
    Value,
};

pub struct Client {
    agent: ureq::Agent,
    url: String,
    token: Option<String>,
}

impl Client {
    pub fn new(url: &str, token: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            url: format!("{}/api/rpc", url.trim_end_matches('/')),
            token,
        }
    }

    /// Runs one command and returns its result, or the server's error message.
    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let mut req = self.agent.post(&self.url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let mut resp = req
            .send_json(json!({ "method": method, "params": params }))
            .map_err(|e| anyhow!("cannot reach reclass-server at {} ({e})", self.url))?;
        match resp.status().as_u16() {
            200 => {}
            401 => bail!(match self.token {
                Some(_) => "the server rejected the API token in RECLASS_API_TOKEN",
                None => "no API token: set RECLASS_API_TOKEN to the token reclass-server printed",
            }),
            403 => bail!("the server rejected this request's origin"),
            status => bail!("reclass-server answered HTTP {status}"),
        }
        let body: Value = resp.body_mut().read_json()?;
        if body["ok"] == true {
            Ok(body["result"].clone())
        } else {
            bail!("{}", body["error"].as_str().unwrap_or("unknown error"))
        }
    }
}

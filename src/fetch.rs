use std::time::Duration;

use anyhow::{Context, Result};

use crate::amazon::{Market, PageKind, classify};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";

#[derive(Debug)]
pub enum FetchError {
    Blocked(String),
    NotFound(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Blocked(why) => write!(f, "blocked by Amazon: {why}"),
            FetchError::NotFound(url) => write!(
                f,
                "page not found: {url} (Amazon also serves this page to clients it distrusts)"
            ),
        }
    }
}

impl std::error::Error for FetchError {}

pub struct Fetcher {
    pub verbose: bool,
    agent: ureq::Agent,
}

impl Fetcher {
    pub fn new(verbose: bool) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self { verbose, agent }
    }

    pub fn amazon(&self, url: &str, market: Market) -> Result<String> {
        if self.verbose {
            eprintln!("shop: GET {url}");
        }
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", USER_AGENT)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", market.accept_language())
            .header("Upgrade-Insecure-Requests", "1")
            .call()
            .with_context(|| format!("GET {url}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_to_string()
            .with_context(|| format!("reading {url}"))?;
        if self.verbose {
            eprintln!("shop: HTTP {status}, {} bytes", body.len());
        }
        if status == 404 {
            return Err(FetchError::NotFound(url.to_string()).into());
        }
        match classify(&body) {
            PageKind::Ok if status < 400 => Ok(body),
            PageKind::Ok => Err(FetchError::Blocked(format!("HTTP {status}")).into()),
            PageKind::Blocked(why) => Err(FetchError::Blocked(why.to_string()).into()),
            PageKind::NotFound => Err(FetchError::NotFound(url.to_string()).into()),
        }
    }

    pub fn json(&self, url: &str, headers: &[(&str, String)]) -> Result<(u16, String)> {
        let mut req = self.agent.get(url).header("Accept", "application/json");
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        let mut resp = req.call().context("calling the Rakuten API")?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string()?;
        Ok((status, body))
    }
}

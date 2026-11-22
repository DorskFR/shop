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
            FetchError::Blocked(why) => write!(f, "blocked by {why}"),
            FetchError::NotFound(what) => write!(f, "not found: {what}"),
        }
    }
}

impl std::error::Error for FetchError {}

pub struct Response {
    pub status: u16,
    pub body: String,
}

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
        let url = &with_origin(url, std::env::var("SHOP_AMAZON_ORIGIN").ok().as_deref());
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
        let not_found = || {
            FetchError::NotFound(format!(
                "{url} (Amazon also serves this page to clients it distrusts)"
            ))
        };
        if status == 404 {
            return Err(not_found().into());
        }
        match classify(&body) {
            PageKind::Ok if status < 400 => Ok(body),
            PageKind::Ok => Err(FetchError::Blocked(format!("Amazon: HTTP {status}")).into()),
            PageKind::Blocked(why) => Err(FetchError::Blocked(format!("Amazon: {why}")).into()),
            PageKind::NotFound => Err(not_found().into()),
        }
    }

    pub fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response> {
        if self.verbose {
            eprintln!("shop: GET {url}");
        }
        let mut req = self.agent.get(url).header("User-Agent", USER_AGENT);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut resp = req.call().with_context(|| format!("GET {url}"))?;
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
        Ok(Response { status, body })
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

pub fn with_origin(url: &str, origin: Option<&str>) -> String {
    let Some(origin) = origin.filter(|o| !o.is_empty()) else {
        return url.to_string();
    };
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map_or("/", |i| &rest[i..]);
    format!("{}{path}", origin.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::with_origin;

    #[test]
    fn origin_override() {
        let u = "https://www.amazon.co.jp/s?k=x";
        assert_eq!(with_origin(u, None), u);
        assert_eq!(
            with_origin(u, Some("http://127.0.0.1:8765/")),
            "http://127.0.0.1:8765/s?k=x"
        );
    }
}

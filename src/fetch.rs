use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use crate::amazon::{Market, PageKind, classify};

pub const UA_CHROME_MAJOR: &str = "142";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum FetchMode {
    Auto,
    Http,
    Browser,
}

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
    pub mode: FetchMode,
    pub verbose: bool,
    agent: ureq::Agent,
}

impl Fetcher {
    pub fn new(mode: FetchMode, verbose: bool) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        Self {
            mode,
            verbose,
            agent,
        }
    }

    pub fn amazon(&self, url: &str, market: Market) -> Result<String> {
        let http = || self.amazon_http(url, market);
        let browser = || self.amazon_browser(url, market);
        match self.mode {
            FetchMode::Http => http(),
            FetchMode::Browser => browser(),
            FetchMode::Auto => match http() {
                Err(e) if self.should_retry(&e, market) => {
                    if self.verbose {
                        eprintln!("shop: {e}, retrying with headless Chrome");
                    }
                    match find_chrome() {
                        Some(_) => browser(),
                        None => Err(
                            e.context("no Chrome found for the browser fallback (set SHOP_CHROME)")
                        ),
                    }
                }
                other => other,
            },
        }
    }

    fn should_retry(&self, e: &anyhow::Error, market: Market) -> bool {
        match e.downcast_ref::<FetchError>() {
            Some(FetchError::Blocked(_)) => true,
            Some(FetchError::NotFound(_)) => market == Market::Com,
            None => false,
        }
    }

    fn amazon_http(&self, url: &str, market: Market) -> Result<String> {
        if self.verbose {
            eprintln!("shop: GET {url}");
        }
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", &user_agent(UA_CHROME_MAJOR))
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
        check(url, &body, status)
    }

    fn amazon_browser(&self, url: &str, market: Market) -> Result<String> {
        let chrome = find_chrome()
            .context("no Chrome found (set SHOP_CHROME to a Chrome/Chromium binary)")?;
        let major = chrome_major(&chrome).unwrap_or_else(|| UA_CHROME_MAJOR.to_string());
        if self.verbose {
            eprintln!("shop: {} --dump-dom {url}", chrome.display());
        }
        let lang = match market {
            Market::Jp => "ja-JP",
            Market::Com => "en-US",
        };
        let args = [
            "--headless=new".to_string(),
            "--disable-gpu".into(),
            "--hide-scrollbars".into(),
            "--mute-audio".into(),
            "--virtual-time-budget=8000".into(),
            format!("--lang={lang}"),
            format!("--user-agent={}", user_agent(&major)),
            "--dump-dom".into(),
            url.into(),
        ];
        let run = |no_sandbox: bool| {
            let mut cmd = Command::new(&chrome);
            cmd.args(&args);
            if no_sandbox {
                cmd.arg("--no-sandbox");
            }
            cmd.output()
                .with_context(|| format!("running {}", chrome.display()))
        };
        let mut out = run(std::env::var_os("SHOP_CHROME_NO_SANDBOX").is_some())?;
        if !out.status.success()
            && String::from_utf8_lossy(&out.stderr).contains("No usable sandbox")
        {
            if self.verbose {
                eprintln!("shop: chrome has no usable sandbox here, retrying with --no-sandbox");
            }
            out = run(true)?;
        }
        if !out.status.success() {
            bail!(
                "chrome exited with {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .find(|l| l.contains("FATAL") || l.contains("ERROR"))
                    .unwrap_or("")
            );
        }
        let body = String::from_utf8_lossy(&out.stdout).into_owned();
        if self.verbose {
            eprintln!("shop: chrome returned {} bytes", body.len());
        }
        check(url, &body, 200)
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

fn check(url: &str, body: &str, status: u16) -> Result<String> {
    match classify(body) {
        PageKind::Ok if status < 400 || status == 404 => Ok(body.to_string()),
        PageKind::Ok => Err(FetchError::Blocked(format!("HTTP {status}")).into()),
        PageKind::Blocked(why) => Err(FetchError::Blocked(why.to_string()).into()),
        PageKind::NotFound => Err(FetchError::NotFound(url.to_string()).into()),
    }
}

fn user_agent(major: &str) -> String {
    format!(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36"
    )
}

fn chrome_major(chrome: &Path) -> Option<String> {
    let out = Command::new(chrome).arg("--version").output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout);
    let major: String = v
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!major.is_empty()).then_some(major)
}

pub fn find_chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SHOP_CHROME") {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in [
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Some(p) = std::env::split_paths(&path)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
        {
            return Some(p);
        }
    }
    let mac = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    if mac.is_file() {
        return Some(mac);
    }
    let cache = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/ms-playwright"))
        })?;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&cache)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("chromium-"))
        })
        .collect();
    dirs.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.trim_start_matches("chromium-").parse::<u32>().ok())
            .unwrap_or(0)
    });
    dirs.into_iter().rev().find_map(|d| {
        ["chrome-linux64/chrome", "chrome-linux/chrome"]
            .iter()
            .map(|rel| d.join(rel))
            .find(|p| p.is_file())
    })
}

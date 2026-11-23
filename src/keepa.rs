use anyhow::{Result, bail};
use serde::Serialize;

use crate::amazon::Market;
use crate::fetch::{FetchError, Fetcher};

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Debug)]
pub struct ChartOptions {
    pub range: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct PriceChart {
    pub asin: String,
    pub market: Market,
    pub range: u32,
    pub path: String,
    pub chart_url: String,
    pub keepa_url: String,
}

pub fn domain(market: Market) -> &'static str {
    match market {
        Market::Jp => "co.jp",
        Market::Com => "com",
    }
}

pub fn chart_url(market: Market, asin: &str, opts: ChartOptions) -> String {
    let mut url = url::Url::parse("https://graph.keepa.com/pricehistory.png").expect("static url");
    url.query_pairs_mut()
        .append_pair("asin", asin)
        .append_pair("domain", domain(market))
        .append_pair("range", &opts.range.to_string())
        .append_pair("amazon", "1")
        .append_pair("new", "1")
        .append_pair("width", &opts.width.to_string())
        .append_pair("height", &opts.height.to_string());
    url.into()
}

pub fn product_url(market: Market, asin: &str) -> String {
    let id = match market {
        Market::Jp => 5,
        Market::Com => 1,
    };
    format!("https://keepa.com/#!product/{id}-{asin}")
}

pub fn default_path(asin: &str, range: u32) -> String {
    format!("{asin}-{range}d.png")
}

pub fn is_png(body: &[u8]) -> bool {
    body.starts_with(PNG_MAGIC)
}

pub fn check(
    asin: &str,
    status: u16,
    content_type: Option<&str>,
    screenshot_status: Option<&str>,
    body: &[u8],
) -> Result<()> {
    match status {
        200 => {}
        403 | 429 | 503 => {
            return Err(FetchError::Blocked(format!("Keepa: HTTP {status}")).into());
        }
        _ => bail!("Keepa chart request failed: HTTP {status}"),
    }
    if !is_png(body) {
        bail!(
            "Keepa returned {} instead of a PNG chart (endpoint changed or rate-limited)",
            content_type.unwrap_or("an unknown content type")
        );
    }
    if screenshot_status.is_some_and(|s| s.eq_ignore_ascii_case("FAIL")) {
        return Err(FetchError::NotFound(format!("Keepa has no price history for {asin}")).into());
    }
    Ok(())
}

pub fn fetch_chart(
    fetcher: &Fetcher,
    market: Market,
    asin: &str,
    opts: ChartOptions,
) -> Result<Vec<u8>> {
    let resp = fetcher.get_bytes(&chart_url(market, asin, opts), &[("Accept", "image/png")])?;
    check(
        asin,
        resp.status,
        resp.header("content-type"),
        resp.header("screenshot-status"),
        &resp.body,
    )?;
    Ok(resp.body)
}

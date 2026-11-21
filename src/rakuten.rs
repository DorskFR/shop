use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;

use crate::fetch::Fetcher;

pub const ENDPOINT: &str = "https://openapi.rakuten.co.jp/ichibams/api/IchibaItem/Search/20260701";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sort {
    Standard,
    PriceAsc,
    PriceDesc,
    Reviews,
    Rating,
    Newest,
}

impl Sort {
    fn param(self) -> &'static str {
        match self {
            Sort::Standard => "standard",
            Sort::PriceAsc => "+itemPrice",
            Sort::PriceDesc => "-itemPrice",
            Sort::Reviews => "-reviewCount",
            Sort::Rating => "-reviewAverage",
            Sort::Newest => "-updateTimestamp",
        }
    }
}

pub struct Credentials {
    pub app_id: String,
    pub access_key: String,
    pub referer: Option<String>,
}

#[derive(Default)]
pub struct Query<'a> {
    pub keyword: Option<&'a str>,
    pub item_code: Option<&'a str>,
    pub page: u32,
    pub hits: u32,
    pub sort: Option<Sort>,
    pub min_price: Option<u64>,
    pub max_price: Option<u64>,
    pub in_stock: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchPage {
    pub count: u64,
    pub page: u64,
    pub page_count: u64,
    pub items: Vec<Item>,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub item_code: String,
    pub name: String,
    pub url: String,
    pub price: u64,
    pub shop: String,
    pub shop_code: String,
    pub rating: f64,
    pub rating_count: u64,
    pub in_stock: bool,
    pub postage_included: bool,
    pub point_rate: u64,
    pub catchcopy: Option<String>,
    pub caption: Option<String>,
    pub images: Vec<String>,
}

pub fn build_url(creds: &Credentials, q: &Query) -> String {
    let mut url = url::Url::parse(ENDPOINT).expect("static url");
    {
        let mut p = url.query_pairs_mut();
        p.append_pair("applicationId", &creds.app_id);
        p.append_pair("format", "json");
        p.append_pair("formatVersion", "2");
        if let Some(k) = q.keyword {
            p.append_pair("keyword", k);
        }
        if let Some(c) = q.item_code {
            p.append_pair("itemCode", c);
        }
        if q.page > 1 {
            p.append_pair("page", &q.page.to_string());
        }
        if q.hits > 0 {
            p.append_pair("hits", &q.hits.min(30).to_string());
        }
        if let Some(s) = q.sort {
            p.append_pair("sort", s.param());
        }
        if let Some(v) = q.min_price {
            p.append_pair("minPrice", &v.to_string());
        }
        if let Some(v) = q.max_price {
            p.append_pair("maxPrice", &v.to_string());
        }
        if q.in_stock {
            p.append_pair("availability", "1");
        }
    }
    url.into()
}

pub fn search(fetcher: &Fetcher, creds: &Credentials, q: &Query) -> Result<SearchPage> {
    let url = build_url(creds, q);
    let mut headers = vec![("accessKey", creds.access_key.clone())];
    if let Some(r) = &creds.referer {
        headers.push(("Referer", r.clone()));
        headers.push(("Origin", r.trim_end_matches('/').to_string()));
    }
    if fetcher.verbose {
        eprintln!("shop: GET {ENDPOINT}");
    }
    let (status, body) = fetcher.json(&url, &headers)?;
    parse_response(status, &body)
}

pub fn parse_response(status: u16, body: &str) -> Result<SearchPage> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("Rakuten returned HTTP {status} with a non-JSON body: {e}"))?;
    if let Some(msg) = error_message(&v) {
        bail!("Rakuten API error (HTTP {status}): {msg}");
    }
    if status >= 400 {
        bail!("Rakuten API returned HTTP {status}");
    }
    let raw = v
        .get("Items")
        .or_else(|| v.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let items = raw
        .iter()
        .map(|i| i.get("Item").unwrap_or(i))
        .map(parse_item)
        .collect();
    Ok(SearchPage {
        count: u(&v, "count"),
        page: u(&v, "page"),
        page_count: u(&v, "pageCount"),
        items,
    })
}

fn error_message(v: &Value) -> Option<String> {
    if let Some(e) = v.get("errors") {
        let code = e
            .get("errorCode")
            .map(|c| c.to_string())
            .unwrap_or_default();
        let msg = s(e, "errorMessage").unwrap_or_default();
        return Some(format!("{code} {msg}").trim().to_string());
    }
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        return Some(format!(
            "{e}: {}",
            s(v, "error_description").unwrap_or_default()
        ));
    }
    if v.get("statusCode").is_some() {
        return Some(s(v, "message").unwrap_or_else(|| v.to_string()));
    }
    None
}

fn parse_item(i: &Value) -> Item {
    let images = i
        .get("mediumImageUrls")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_str()
                        .or_else(|| x.get("imageUrl").and_then(Value::as_str))
                })
                .map(|x| x.split('?').next().unwrap_or(x).to_string())
                .collect()
        })
        .unwrap_or_default();
    Item {
        item_code: s(i, "itemCode").unwrap_or_default(),
        name: s(i, "itemName").unwrap_or_default(),
        url: s(i, "itemUrl").unwrap_or_default(),
        price: u(i, "itemPrice"),
        shop: s(i, "shopName").unwrap_or_default(),
        shop_code: s(i, "shopCode").unwrap_or_default(),
        rating: i.get("reviewAverage").and_then(num).unwrap_or(0.0),
        rating_count: u(i, "reviewCount"),
        in_stock: u(i, "availability") == 1,
        postage_included: u(i, "postageFlag") == 0,
        point_rate: u(i, "pointRate"),
        catchcopy: s(i, "catchcopy").filter(|t| !t.is_empty()),
        caption: s(i, "itemCaption").filter(|t| !t.is_empty()),
        images,
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|t| t.parse().ok()))
}

fn u(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(num).map(|f| f as u64).unwrap_or(0)
}

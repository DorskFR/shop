use anyhow::{Context, Result, anyhow};
use scraper::{Html, Node};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    product::{Connector, Product, Query, SearchPage, Sort},
};

const API: &str = "https://www.nitori-net.jp/occ/v2/nitorinet/nitori/products";
const SITE: &str = "https://www.nitori-net.jp/ec/product/";
const DEFAULT_PAGE_SIZE: u32 = 24;
const ATTEMPTS: usize = 2;

pub struct Nitori;

pub fn sort_param(sort: Sort) -> &'static str {
    match sort {
        Sort::Relevance => "relevance",
        Sort::PriceAsc => "price-asc",
        Sort::PriceDesc => "price-desc",
        Sort::Reviews => "ratingCount-desc",
        Sort::Newest => "effective_date-desc",
    }
}

pub fn search_url(query: &Query) -> Result<String> {
    let sort = sort_param(query.sort);
    let text = query.text.replace(':', " ");
    let mut url = url::Url::parse(&format!("{API}/search"))?;
    url.query_pairs_mut()
        .append_pair("query", text.trim())
        .append_pair("sort", sort)
        .append_pair(
            "currentPage",
            &query.page.max(1).saturating_sub(1).to_string(),
        )
        .append_pair(
            "pageSize",
            &query
                .limit
                .unwrap_or(DEFAULT_PAGE_SIZE)
                .clamp(1, 60)
                .to_string(),
        )
        .append_pair("lang", "ja")
        .append_pair("curr", "JPY");
    Ok(url.into())
}

pub fn item_url(id: &str) -> String {
    format!("{API}/{id}?lang=ja&curr=JPY")
}

pub fn product_url(id: &str) -> String {
    format!("{SITE}{id}/")
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    let candidate = match input.find("/product/") {
        Some(i) => input[i + "/product/".len()..]
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(""),
        None => input,
    };
    let valid = !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        && candidate.chars().any(|c| c.is_ascii_digit());
    valid.then(|| candidate.to_string())
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn yen(v: &Value) -> Option<u64> {
    let n = v.as_f64()?;
    (n >= 0.0).then(|| n.round() as u64)
}

fn count(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().map(|n| n as u64))
}

fn rating(v: &Value, reviews: Option<u64>) -> Option<f64> {
    let r = v.as_f64()?;
    (reviews.unwrap_or(0) > 0 && r > 0.0).then(|| (r * 100.0).round() / 100.0)
}

fn push_unique(list: &mut Vec<String>, s: String) {
    if !s.is_empty() && !list.contains(&s) {
        list.push(s);
    }
}

fn html_to_text(html: &str) -> Option<String> {
    let frag = Html::parse_fragment(html);
    let mut out = String::new();
    for node in frag.tree.nodes() {
        match node.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if matches!(e.name(), "br" | "p" | "li" | "div") => out.push('\n'),
            _ => {}
        }
    }
    let lines: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("ARについての詳細"))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub fn parse_search(body: &str) -> Result<SearchPage> {
    let v: Value = serde_json::from_str(body).context("Nitori: search response is not JSON")?;
    if let Some(msg) = api_error(&v) {
        return Err(anyhow!("Nitori: search failed: {msg}"));
    }
    let pagination = &v["pagination"];
    let items = v["products"]
        .as_array()
        .map(|a| a.iter().filter_map(parse_search_item).collect())
        .unwrap_or_default();
    Ok(SearchPage {
        total: pagination["totalResults"].as_u64(),
        page: pagination["currentPage"].as_u64().unwrap_or(0) as u32 + 1,
        items,
    })
}

fn parse_search_item(p: &Value) -> Option<Product> {
    let id = text(&p["code"])?;
    let reviews = count(&p["reviewCount"]);
    let mut badges = Vec::new();
    if p["discountFlg"].as_u64().unwrap_or(0) == 2 {
        badges.push("期間限定価格".to_string());
    }
    for icon in p["iconImageAltTexts"].as_array().into_iter().flatten() {
        if let Some(t) = text(&icon["altText"]) {
            push_unique(&mut badges, t);
        }
    }
    let image =
        text(&p["images"][0]["url"]).or_else(|| text(&p["images"]["PRIMARY"]["product"]["url"]));
    Some(Product {
        url: product_url(&id),
        name: text(&p["name"]).or_else(|| text(&p["summary"]))?,
        brand: text(&p["brand"]["name"]),
        price: yen(&p["price"]["value"]),
        rating: rating(&p["averageRating"], reviews),
        rating_count: reviews,
        in_stock: Some(true),
        badges,
        image,
        id,
        ..Product::default()
    })
}

fn api_error(v: &Value) -> Option<String> {
    let e = v["errors"].as_array()?.first()?;
    Some(
        text(&e["message"])
            .or_else(|| text(&e["type"]))
            .unwrap_or_else(|| "unknown error".to_string()),
    )
}

pub fn parse_item(body: &str) -> Result<Product> {
    let v: Value = serde_json::from_str(body).context("Nitori: item response is not JSON")?;
    if let Some(msg) = api_error(&v) {
        let kind = v["errors"][0]["type"].as_str().unwrap_or("");
        if kind.contains("Unknown")
            || kind.contains("NotFound")
            || msg.contains("not found")
            || msg.contains("No Base Product")
        {
            return Err(FetchError::NotFound(format!("Nitori product ({msg})")).into());
        }
        return Err(anyhow!("Nitori: product lookup failed: {msg}"));
    }
    let id = text(&v["code"])
        .ok_or_else(|| FetchError::NotFound("Nitori product (no code in response)".into()))?;
    let sku = &v["skuData"];
    let reviews = count(&v["numberOfReviews"]);

    let mut variants = Vec::new();
    for var in v["variations"].as_array().into_iter().flatten() {
        let label = |x: &Value| match (text(&x["masterName"]), text(&x["name"])) {
            (Some(k), Some(n)) => Some(format!("{k}: {n}")),
            (None, Some(n)) => Some(n),
            _ => None,
        };
        if let Some(s) = label(var) {
            push_unique(&mut variants, s);
        }
        for sub in var["subVariationsList"].as_array().into_iter().flatten() {
            if let Some(s) = label(sub) {
                push_unique(&mut variants, s);
            }
        }
    }

    let mut categories = Vec::new();
    let cat = &v["categories"];
    for sup in cat["supercategories"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
    {
        if let Some(n) = text(&sup["name"]) {
            push_unique(&mut categories, n);
        }
    }
    if let Some(n) = text(&cat["name"]) {
        push_unique(&mut categories, n);
    }

    let mut details = Vec::new();
    const SPEC_LABELS: [(&str, &str); 9] = [
        ("size", "サイズ"),
        ("color", "カラー"),
        ("material", "素材"),
        ("weight", "重さ"),
        ("packingSize", "梱包サイズ"),
        ("assemblyTime", "組立時間"),
        ("assuranceYears", "保証期間"),
        ("countryOfOrigin", "原産国"),
        ("bedHeight", "ベッド高さ"),
    ];
    for (key, label) in SPEC_LABELS {
        if let Some(mut val) = text(&sku["specifications"][key]) {
            if key == "assuranceYears" && val.chars().all(|c| c.is_ascii_digit()) {
                val.push('年');
            }
            details.push((label.to_string(), val));
        }
    }
    if let Some(val) = text(&sku["specifications"]["chairHeight"]) {
        details.push(("座面高さ".to_string(), val));
    }
    let delivery = &sku["delivery"];
    for (key, label) in [
        ("shippingLabel1", "送料"),
        ("deliveryMethod1", "お届け方法"),
        ("returnsExchanges", "返品・交換"),
        ("storeReceipt", "店舗受取"),
    ] {
        if let Some(val) = text(&delivery[key]) {
            details.push((label.to_string(), val));
        }
    }

    let mut badges = Vec::new();
    if sku["discountFlag"].as_u64().unwrap_or(0) == 2 {
        badges.push("期間限定価格".to_string());
    }

    let description = text(&sku["productDescription"])
        .or_else(|| text(&v["productDescription"]))
        .and_then(|d| html_to_text(&d));
    let image = text(&sku["mediasList"][0]["url"]).or_else(|| text(&v["mediasList"][0]["url"]));

    Ok(Product {
        url: product_url(&id),
        name: text(&sku["name"])
            .or_else(|| text(&sku["catchCopy"]))
            .unwrap_or_else(|| id.clone()),
        brand: text(&v["brand"]["name"]),
        price: yen(&v["price"]["value"]),
        list_price: None,
        rating: rating(&v["averageRating"], reviews),
        rating_count: reviews,
        in_stock: None,
        badges,
        variants,
        categories,
        details,
        description,
        image,
        id,
    })
}

fn fetch_json(fetcher: &Fetcher, url: &str) -> Result<String> {
    let headers = [
        ("Accept", "application/json"),
        ("Accept-Language", "ja,en;q=0.8"),
        ("Referer", "https://www.nitori-net.jp/ec/"),
    ];
    let mut last = anyhow!("Nitori: no attempt made");
    for _ in 0..ATTEMPTS {
        match fetcher.get(url, &headers) {
            Ok(r) if r.status == 403 => {
                return Err(FetchError::Blocked("Nitori: HTTP 403 from Akamai".into()).into());
            }
            Ok(r) if r.status >= 500 => {
                last = anyhow!(
                    "Nitori: HTTP {} (the shop's API is overloaded or in maintenance)",
                    r.status
                );
            }
            Ok(r) if r.body.trim_start().starts_with('<') && r.status < 400 => {
                return Err(
                    FetchError::Blocked("Nitori: got an HTML page instead of JSON".into()).into(),
                );
            }
            Ok(r) => return Ok(r.body),
            Err(e) => last = e,
        }
    }
    Err(last)
}

impl Connector for Nitori {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let body = fetch_json(fetcher, &search_url(query)?)?;
        let mut page = parse_search(&body)?;
        page.page = query.page.max(1);
        Ok(page)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let id = parse_id(id_or_url)
            .ok_or_else(|| anyhow!("not a Nitori product id or URL: {id_or_url}"))?;
        let body = fetch_json(fetcher, &item_url(&id))?;
        parse_item(&body)
    }
}

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    product::{Connector, Product, Query, SearchPage, Sort},
};

pub struct Ikea;

const SEARCH_ENDPOINT: &str = "https://sik.search.blue.cdtapps.com/jp/ja/search-result-page";
const AVAILABILITY_ENDPOINT: &str = "https://api.salesitem.ingka.com/availabilities/ru/jp";
const ITEM_BASE: &str = "https://www.ikea.com/jp/ja/p/";
const DEFAULT_CLIENT_KEY: &str = "ef382663-a2a5-40d4-8afe-f0634821c0ed";
const DEFAULT_PAGE_SIZE: u32 = 24;
const MAX_FETCH: u32 = 500;
const AVAILABILITY_CHUNK: usize = 30;

impl Connector for Ikea {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let url = search_url(query)?;
        let resp = fetcher.get(&url, &[("Accept", "application/json")])?;
        check_status("search", resp.status, &resp.body)?;
        let mut page = parse_search(&resp.body)?;
        let (_, skip, take) = page_window(query);
        page.items = page.items.into_iter().skip(skip).take(take).collect();
        page.page = query.page.max(1);
        if let Ok(stock) = fetch_stock(fetcher, DEFAULT_CLIENT_KEY, &item_nos(&page.items)) {
            for p in &mut page.items {
                p.in_stock = stock.get(item_no(&p.id)).copied();
            }
        }
        Ok(page)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let id = parse_id(id_or_url).with_context(|| {
            format!("not an IKEA item number (8 digits) or ikea.com/jp product URL: {id_or_url}")
        })?;
        let resp = fetcher.get(
            &item_url(&id),
            &[
                ("Accept", "text/html,application/xhtml+xml"),
                ("Accept-Language", "ja-JP,ja;q=0.9"),
            ],
        )?;
        if resp.status == 404 {
            return Err(FetchError::NotFound(format!("IKEA item {id}")).into());
        }
        check_status("item page", resp.status, &resp.body)?;
        let mut product = parse_item(&resp.body, &id)?;
        let key = client_key(&resp.body).unwrap_or_else(|| DEFAULT_CLIENT_KEY.to_string());
        if let Ok(stock) = fetch_stock(fetcher, &key, &[item_no(&id).to_string()])
            && let Some(v) = stock.get(item_no(&id))
        {
            product.in_stock = Some(*v);
        }
        Ok(product)
    }
}

pub fn check_status(what: &str, status: u16, body: &str) -> Result<()> {
    match status {
        200..=299 => Ok(()),
        403 | 429 => Err(FetchError::Blocked(format!("IKEA: HTTP {status} on {what}")).into()),
        _ => {
            let detail = serde_json::from_str::<Value>(body)
                .ok()
                .and_then(|v| v.get("detail").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| body.chars().take(120).collect());
            bail!("IKEA {what} returned HTTP {status}: {detail}")
        }
    }
}

fn sort_param(sort: Sort) -> Option<&'static str> {
    match sort {
        Sort::Relevance => None,
        Sort::PriceAsc => Some("PRICE_LOW_TO_HIGH"),
        Sort::PriceDesc => Some("PRICE_HIGH_TO_LOW"),
        Sort::Reviews => Some("RATING"),
        Sort::Newest => Some("NEWEST"),
    }
}

/// The search API has no offset parameter, so a page is cut out of a larger result.
/// Returns (size to request, items to skip, items to keep).
pub fn page_window(query: &Query) -> (u32, usize, usize) {
    let per_page = query.limit.unwrap_or(DEFAULT_PAGE_SIZE).max(1);
    let page = query.page.max(1);
    let size = per_page.saturating_mul(page).min(MAX_FETCH);
    (
        size,
        (per_page as usize) * (page as usize - 1),
        per_page as usize,
    )
}

pub fn search_url(query: &Query) -> Result<String> {
    let (size, _, _) = page_window(query);
    let size = size.to_string();
    let mut params = vec![
        ("q", query.text),
        ("types", "PRODUCT"),
        ("size", size.as_str()),
    ];
    if let Some(s) = sort_param(query.sort) {
        params.push(("sort", s));
    }
    Ok(url::Url::parse_with_params(SEARCH_ENDPOINT, params)?.into())
}

pub fn item_url(id: &str) -> String {
    format!("{ITEM_BASE}-{id}/")
}

fn item_no(id: &str) -> &str {
    id.strip_prefix('s').unwrap_or(id)
}

fn item_nos(items: &[Product]) -> Vec<String> {
    items.iter().map(|p| item_no(&p.id).to_string()).collect()
}

fn normalize_id(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '.' | ' ' | '-' | '\u{3000}'))
        .collect::<String>()
        .to_lowercase();
    let digits = cleaned.strip_prefix('s').unwrap_or(&cleaned);
    (digits.len() == 8 && digits.chars().all(|c| c.is_ascii_digit())).then_some(cleaned)
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    if input.contains('/') || input.contains("ikea.com") {
        let with_scheme = if input.contains("://") {
            input.to_string()
        } else {
            format!("https://{}", input.trim_start_matches('/'))
        };
        let url = url::Url::parse(&with_scheme).ok()?;
        let host = url.host_str()?;
        if !(host == "ikea.com" || host.ends_with(".ikea.com")) {
            return None;
        }
        let mut segs = url.path_segments()?;
        segs.find(|s| *s == "p")?;
        let slug = segs.next()?;
        let tail = slug.rsplit('-').next().unwrap_or(slug);
        return normalize_id(tail);
    }
    normalize_id(input)
}

fn digits(s: &str) -> Option<u64> {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

fn clean(s: &str) -> String {
    s.replace('\u{200b}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_search(body: &str) -> Result<SearchPage> {
    let v: Value = serde_json::from_str(body).context("IKEA search returned non-JSON")?;
    let main = v
        .pointer("/searchResultPage/products/main")
        .context("IKEA search response has no product list (API changed?)")?;
    let items = main
        .get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| i.get("product"))
                .filter_map(search_product)
                .collect()
        })
        .unwrap_or_default();
    Ok(SearchPage {
        total: main.get("max").and_then(Value::as_u64),
        page: 1,
        items,
    })
}

fn search_product(p: &Value) -> Option<Product> {
    let s = |k: &str| p.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    let id = s("id")?.to_string();
    let mut name = clean(&format!(
        "{} {}",
        s("name").unwrap_or_default(),
        s("typeName").unwrap_or_default()
    ));
    if let Some(design) = s("validDesignText") {
        name = format!("{name}, {}", clean(design));
    }
    let price = p.get("salesPrice");
    let numeral = |v: Option<&Value>| {
        v.and_then(|v| v.get("numeral"))
            .and_then(Value::as_f64)
            .map(|n| n.round() as u64)
    };
    let list_price = price
        .and_then(|p| p.pointer("/previous/wholeNumber"))
        .and_then(Value::as_str)
        .and_then(digits);
    let mut badges = Vec::new();
    if let Some(t) = p.pointer("/badge/text").and_then(Value::as_str) {
        badges.push(t.to_string());
    }
    if let Some(t) = price.and_then(|p| p.get("tagText")).and_then(Value::as_str) {
        badges.push(t.to_string());
    }
    if p.get("lastChance").and_then(Value::as_bool) == Some(true) {
        badges.push("Last chance".to_string());
    }
    let variants = p
        .pointer("/gprDescription/variants")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.get("name").and_then(Value::as_str).map(|_| v))
                .filter_map(|v| v.get("mainImageAlt").and_then(Value::as_str))
                .filter_map(|alt| alt.rsplit_once(", ").map(|(_, d)| clean(d)))
                .collect()
        })
        .unwrap_or_default();
    Some(Product {
        url: s("pipUrl")
            .map(str::to_string)
            .unwrap_or_else(|| item_url(&id)),
        id,
        name,
        brand: Some("IKEA".to_string()),
        price: numeral(price),
        list_price,
        rating: p.get("ratingValue").and_then(Value::as_f64),
        rating_count: p.get("ratingCount").and_then(Value::as_u64),
        in_stock: None,
        badges,
        variants,
        categories: p
            .get("categoryPath")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.get("name").and_then(Value::as_str))
                    .map(clean)
                    .collect()
            })
            .unwrap_or_default(),
        details: Vec::new(),
        description: None,
        image: s("mainImageUrl").map(str::to_string),
    })
}

pub fn parse_availability(body: &str) -> HashMap<String, bool> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return HashMap::new();
    };
    v.get("availabilities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let no = a.pointer("/itemKey/itemNo")?.as_str()?;
            let flag = |k: &str| a.get(k).and_then(Value::as_bool);
            let home = flag("availableForHomeDelivery");
            let collect = flag("availableForClickCollect");
            if home.is_none() && collect.is_none() {
                return None;
            }
            Some((no.to_string(), home == Some(true) || collect == Some(true)))
        })
        .collect()
}

fn fetch_stock(fetcher: &Fetcher, key: &str, nos: &[String]) -> Result<HashMap<String, bool>> {
    let mut out = HashMap::new();
    for chunk in nos.chunks(AVAILABILITY_CHUNK) {
        let url = format!("{AVAILABILITY_ENDPOINT}?itemNos={}", chunk.join(","));
        let resp = fetcher.get(
            &url,
            &[("Accept", "application/json"), ("x-client-id", key)],
        )?;
        if resp.status != 200 {
            bail!("IKEA availability returned HTTP {}", resp.status);
        }
        out.extend(parse_availability(&resp.body));
    }
    Ok(out)
}

fn client_key(html: &str) -> Option<String> {
    let rest = html.split("\"ciaApiClientKey\":\"").nth(1)?;
    let key = rest.split('"').next()?;
    (key.len() >= 16 && key.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| key.to_string())
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn text(el: ElementRef) -> String {
    clean(&el.text().collect::<String>())
}

fn first_text(doc: &Html, css: &str) -> Option<String> {
    doc.select(&sel(css)).map(text).find(|t| !t.is_empty())
}

fn ld_product(doc: &Html) -> Option<Value> {
    doc.select(&sel("script[type=\"application/ld+json\"]"))
        .filter_map(|s| serde_json::from_str::<Value>(&s.text().collect::<String>()).ok())
        .flat_map(|v| match v {
            Value::Array(a) => a,
            other => vec![other],
        })
        .find(|v| v.get("@type").and_then(Value::as_str) == Some("Product"))
}

fn ld_categories(doc: &Html) -> Vec<String> {
    let crumbs = doc
        .select(&sel("script[type=\"application/ld+json\"]"))
        .filter_map(|s| serde_json::from_str::<Value>(&s.text().collect::<String>()).ok())
        .find(|v| v.get("@type").and_then(Value::as_str) == Some("BreadcrumbList"));
    crumbs
        .and_then(|c| c.get("itemListElement").and_then(Value::as_array).cloned())
        .map(|a| {
            a.iter()
                .skip(1)
                .filter_map(|i| i.get("name").and_then(Value::as_str))
                .map(clean)
                .collect()
        })
        .unwrap_or_default()
}

fn as_u64(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::String(s) => digits(s),
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.round() as u64)),
        _ => None,
    }
}

fn as_f64(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::String(s) => s.trim().parse().ok(),
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

pub fn parse_item(html: &str, id: &str) -> Result<Product> {
    let doc = Html::parse_document(html);
    let Some(ld) = ld_product(&doc) else {
        return Err(FetchError::NotFound(format!("IKEA item {id} (no product page)")).into());
    };

    let heading = doc.select(&sel("h1")).next().map(|h| {
        let part = |css: &str| h.select(&sel(css)).next().map(text).unwrap_or_default();
        clean(&format!(
            "{} {}",
            part(".pipcom-price-module__name-decorator"),
            part(".pipcom-price-module__description")
        ))
    });
    let name = heading
        .filter(|n| !n.is_empty())
        .or_else(|| ld.get("name").and_then(Value::as_str).map(clean))
        .unwrap_or_default();

    let offer = match ld.get("offers") {
        Some(Value::Array(a)) => a.first(),
        other => other,
    };
    let price = first_text(
        &doc,
        ".pipcom-price-module__current-price .pipcom-price__integer",
    )
    .and_then(|t| digits(&t))
    .or_else(|| as_u64(offer.and_then(|o| o.get("price"))));
    let list_price = doc
        .select(&sel(".pipcom-price-module__addon"))
        .map(text)
        .find(|t| {
            t.contains('¥')
                && t.contains("価格")
                && !t.contains("消費税")
                && !t.contains("特別価格期間")
        })
        .and_then(|t| digits(t.split_once('¥').map_or(&t, |(_, p)| p)));

    let in_stock = offer
        .and_then(|o| o.get("availability"))
        .and_then(Value::as_str)
        .map(|a| a.ends_with("InStock"));

    let mut badges: Vec<String> = Vec::new();
    let mut badge = |b: String| {
        if !b.is_empty() && !badges.contains(&b) {
            badges.push(b);
        }
    };
    for e in doc.select(&sel(".pipf-product-gallery__commercial-message")) {
        badge(text(e));
    }
    if doc
        .select(&sel(".pipcom-price-module--family"))
        .next()
        .is_some()
    {
        badge("IKEA Family価格".to_string());
    }
    if let Some(t) = first_text(&doc, ".pipcom-price-module__offer-message") {
        badge(t);
    }

    let mut variants = Vec::new();
    for picker in doc.select(&sel(".pipf-product-style-picker")) {
        let title = picker
            .select(&sel(".pipf-product-style-picker__title"))
            .next()
            .map(|t| {
                text(t)
                    .trim_end_matches([':', ' '])
                    .trim_end_matches("を選択")
                    .to_string()
            })
            .unwrap_or_default();
        for item in picker.select(&sel(".pipf-product-style-picker__box [aria-label]")) {
            let label = clean(item.value().attr("aria-label").unwrap_or_default());
            let v = if title.is_empty() {
                label
            } else {
                format!("{title}: {label}")
            };
            if !label_empty(&v) && !variants.contains(&v) {
                variants.push(v);
            }
        }
    }

    let mut details: Vec<(String, String)> = Vec::new();
    if let Some(n) = first_text(&doc, ".pipf-product-identifier__value") {
        details.push(("商品番号".to_string(), n));
    }
    if let Some(n) = first_text(&doc, ".pipf-designer__name") {
        details.push(("デザイナー".to_string(), n));
    }
    for dl in doc.select(&sel(".pipf-product-details-tab__section")) {
        let k = dl.select(&sel("dt")).next().map(text).unwrap_or_default();
        let v = dl.select(&sel("dd")).next().map(text).unwrap_or_default();
        let entry = (k.trim_end_matches(':').trim().to_string(), v);
        if !entry.0.is_empty() && !entry.1.is_empty() && !details.contains(&entry) {
            details.push(entry);
        }
    }
    for row in doc.select(&sel(".pipf-measurements-tab__measurement-row")) {
        let k = row
            .select(&sel(".pipf-measurements-tab__measurement-name"))
            .next()
            .map(text)
            .unwrap_or_default();
        let v = row
            .select(&sel(".pipf-measurements-tab__measurement-value"))
            .next()
            .map(text)
            .unwrap_or_default();
        if !k.is_empty() && !v.is_empty() {
            details.push((k, v));
        }
    }
    if let Some(card) = doc
        .select(&sel(".pipf-measurements-tab__package-measurement-list"))
        .next()
    {
        for li in card.select(&sel("li")) {
            let label = li
                .select(&sel(".pipf-measurements-tab__package-measurement-label"))
                .next()
                .map(text)
                .unwrap_or_default();
            let full = text(li);
            let value = full
                .strip_prefix(&label)
                .unwrap_or(&full)
                .trim()
                .to_string();
            let label = label.trim_end_matches(':').trim();
            if !label.is_empty() && !value.is_empty() {
                details.push((format!("パッケージ {label}"), value));
            }
        }
    }

    let description = first_text(&doc, ".pipf-overview-summary__description").or_else(|| {
        ld.get("description")
            .and_then(Value::as_str)
            .map(clean)
            .filter(|d| !d.is_empty())
    });

    let image = ld
        .get("image")
        .and_then(|i| match i {
            Value::Array(a) => a.first(),
            other => Some(other),
        })
        .and_then(|i| match i {
            Value::String(s) => Some(s.as_str()),
            o => o.get("contentUrl").and_then(Value::as_str),
        })
        .map(str::to_string);

    let url = doc
        .select(&sel("meta[property=\"og:url\"]"))
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(str::to_string)
        .or_else(|| {
            offer
                .and_then(|o| o.get("url"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| item_url(id));

    let rating = ld.get("aggregateRating");
    Ok(Product {
        id: id.to_string(),
        name,
        url,
        brand: ld
            .pointer("/brand/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        price,
        list_price,
        rating: as_f64(rating.and_then(|r| r.get("ratingValue"))),
        rating_count: as_u64(rating.and_then(|r| r.get("reviewCount"))),
        in_stock,
        badges,
        variants,
        categories: ld_categories(&doc),
        details,
        description,
        image,
    })
}

fn label_empty(v: &str) -> bool {
    v.trim_end_matches([':', ' ']).is_empty()
}

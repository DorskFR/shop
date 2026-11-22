use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    product::{Connector, Product, Query, SearchPage, Sort},
};

const API: &str = "https://www.uniqlo.com/jp/api/commerce/v5/ja";
const SITE: &str = "https://www.uniqlo.com/jp/ja/products";
const DEFAULT_LIMIT: u32 = 36;
const MAX_LIMIT: u32 = 100;
const HEADERS: &[(&str, &str)] = &[
    ("Accept", "application/json"),
    ("Accept-Language", "ja-JP,ja;q=0.9"),
    ("x-fr-clientid", "uq.jp.web-spa"),
];

pub struct Uniqlo;

pub fn sort_param(sort: Sort) -> &'static str {
    match sort {
        Sort::Relevance => "1",
        Sort::PriceAsc => "2",
        Sort::PriceDesc => "3",
        Sort::Reviews => "4",
        Sort::Newest => "5",
    }
}

pub fn search_url(query: &Query) -> Result<String> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let offset = query.page.max(1).saturating_sub(1) * limit;
    let mut url = url::Url::parse(&format!("{API}/products")).context("building URL")?;
    url.query_pairs_mut()
        .append_pair("q", query.text)
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair("sort", sort_param(query.sort))
        .append_pair("httpFailure", "true");
    Ok(url.into())
}

pub fn parse_id(input: &str) -> Result<(String, String)> {
    let input = input.trim();
    if let Some((_, rest)) = input.split_once("/products/") {
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let mut parts = rest.split('/').filter(|p| !p.is_empty());
        if let Some(id) = parts.next().filter(|id| valid_id(id)) {
            let group = parts
                .next()
                .filter(|g| g.len() == 2 && g.bytes().all(|b| b.is_ascii_digit()))
                .unwrap_or("00");
            return Ok((id.to_string(), group.to_string()));
        }
    } else if valid_id(input) {
        return Ok((input.to_string(), "00".to_string()));
    } else if !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()) {
        return Ok((format!("E{input}-000"), "00".to_string()));
    }
    bail!("not a Uniqlo product id or URL: {input} (expected e.g. E465185-000)")
}

fn valid_id(id: &str) -> bool {
    match id.rsplit_once('-') {
        Some((head, tail)) => {
            head.len() > 1
                && head.bytes().all(|b| b.is_ascii_alphanumeric())
                && tail.len() == 3
                && tail.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

pub fn product_url(id: &str, group: &str) -> String {
    format!("{SITE}/{id}/{group}")
}

fn parse_json(body: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(body)
        .map_err(|_| FetchError::Blocked("Uniqlo: response was not JSON (bot wall?)".into()))?;
    if v.get("status").and_then(Value::as_str) == Some("ok") {
        return Ok(v);
    }
    let code = v.pointer("/error/code").and_then(Value::as_i64);
    match code {
        Some(25) => Err(FetchError::NotFound("Uniqlo product".into()).into()),
        _ => bail!("Uniqlo API error: {}", v.get("error").unwrap_or(&v)),
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn money(v: &Value) -> Option<u64> {
    v.get("value")
        .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)))
}

fn prices(v: &Value) -> (Option<u64>, Option<u64>) {
    let p = v.get("prices");
    let base = p.and_then(|p| p.get("base")).and_then(money);
    let promo = p.and_then(|p| p.get("promo")).and_then(money);
    match (promo, base) {
        (Some(promo), Some(base)) if promo < base => (Some(promo), Some(base)),
        (Some(promo), _) => (Some(promo), None),
        (None, base) => (base, None),
    }
}

fn rating(v: &Value) -> (Option<f64>, Option<u64>) {
    let r = v.get("rating");
    let count = r.and_then(|r| r.get("count")).and_then(Value::as_u64);
    let avg = r
        .and_then(|r| r.get("average"))
        .and_then(Value::as_f64)
        .filter(|_| count.unwrap_or(0) > 0);
    (avg, count)
}

fn flags(v: &Value) -> Vec<String> {
    let f = v.pointer("/representative/flags");
    let mut out: Vec<String> = Vec::new();
    for key in ["priceFlags", "productFlags"] {
        for flag in f
            .and_then(|f| f.get(key))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(name) = s(flag, "name")
                && !out.contains(&name)
            {
                out.push(name);
            }
        }
    }
    out
}

fn main_image(v: &Value) -> Option<String> {
    let main = v.pointer("/images/main")?.as_object()?;
    let rep = v
        .get("representativeColorDisplayCode")
        .and_then(Value::as_str)
        .or_else(|| {
            v.pointer("/representative/color/displayCode")
                .and_then(Value::as_str)
        });
    rep.and_then(|c| main.get(c))
        .or_else(|| main.values().next())
        .and_then(|m| s(m, "image"))
}

fn names(v: &Value, key: &str) -> Vec<(String, String)> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| Some((s(x, "displayCode")?, s(x, "name")?)))
        .collect()
}

fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.replace("<br>", "\n").replace("<br/>", "\n").chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_search(body: &str, page: u32) -> Result<SearchPage> {
    let v = parse_json(body)?;
    let result = v.get("result").context("Uniqlo: response has no result")?;
    let items = result
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|i| {
            let id = s(i, "productId")?;
            let group = s(i, "priceGroup").unwrap_or_else(|| "00".into());
            let (price, list_price) = prices(i);
            let (rating, rating_count) = rating(i);
            let rep_stock = i.pointer("/representative/sales").and_then(Value::as_bool);
            Some(Product {
                url: product_url(&id, &group),
                name: s(i, "name").unwrap_or_default(),
                brand: Some("UNIQLO".into()),
                price,
                list_price,
                rating,
                rating_count,
                in_stock: rep_stock.filter(|s| !s),
                badges: flags(i),
                variants: variants_summary(i),
                image: main_image(i),
                id,
                ..Product::default()
            })
        })
        .collect();
    Ok(SearchPage {
        total: result.pointer("/pagination/total").and_then(Value::as_u64),
        page,
        items,
    })
}

fn variants_summary(v: &Value) -> Vec<String> {
    let colors: Vec<String> = names(v, "colors").into_iter().map(|(_, n)| n).collect();
    let sizes: Vec<String> = names(v, "sizes").into_iter().map(|(_, n)| n).collect();
    let mut out = Vec::new();
    if !colors.is_empty() {
        out.push(format!("colors: {}", colors.join(", ")));
    }
    if !sizes.is_empty() {
        out.push(format!("sizes: {}", sizes.join(", ")));
    }
    out
}

pub fn parse_item(details: &str, l2s: Option<&str>, group: &str) -> Result<Product> {
    let v = parse_json(details)?;
    let d = v.get("result").context("Uniqlo: response has no result")?;
    let id = s(d, "productId").context("Uniqlo: product has no productId")?;
    let (price, list_price) = prices(d);
    let (rating, rating_count) = rating(d);

    let colors = names(d, "colors");
    let sizes = names(d, "sizes");
    let stock = l2s
        .and_then(|b| parse_json(b).ok())
        .map(|v| v["result"].clone());
    let (variants, in_stock) = match &stock {
        Some(st) => stock_variants(st, &colors, &sizes),
        None => (variants_summary(d), None),
    };

    let mut categories = Vec::new();
    for level in ["gender", "class", "category", "subcategory"] {
        if let Some(n) = d
            .pointer(&format!("/breadcrumbs/{level}"))
            .and_then(|c| s(c, "locale"))
            && !categories.contains(&n)
        {
            categories.push(n);
        }
    }

    let mut details_kv: Vec<(String, String)> = Vec::new();
    let mut add = |k: &str, val: Option<String>| {
        if let Some(val) = val.map(|t| plain(&t)).filter(|t| !t.is_empty()) {
            details_kv.push((k.to_string(), val));
        }
    };
    add("素材", s(d, "composition"));
    add("デザイン", s(d, "designDetail"));
    add("洗濯", s(d, "washingInformation"));
    add("お手入れ", s(d, "careInstruction"));
    add("備考", s(d, "freeInformation"));
    add("サイズ", s(d, "sizeInformation"));
    add("サイズ表", s(d, "sizeChartUrl"));
    let origins: Vec<String> = d
        .get("countriesOfOrigin")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| s(c, "code"))
        .collect();
    if !origins.is_empty() {
        details_kv.push(("原産国".into(), origins.join(", ")));
    }

    let description = [s(d, "shortDescription"), s(d, "longDescription")]
        .into_iter()
        .flatten()
        .map(|t| plain(&t))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let mut badges = flags(d);
    if d.get("isBestSeller").and_then(Value::as_bool) == Some(true) {
        badges.push("ベストセラー".into());
    }

    Ok(Product {
        url: product_url(&id, group),
        name: s(d, "name").unwrap_or_default(),
        brand: Some("UNIQLO".into()),
        price,
        list_price,
        rating,
        rating_count,
        in_stock,
        badges,
        variants,
        categories,
        details: details_kv,
        description: Some(description).filter(|t| !t.is_empty()),
        image: main_image(d),
        id,
    })
}

fn stock_variants(
    st: &Value,
    colors: &[(String, String)],
    sizes: &[(String, String)],
) -> (Vec<String>, Option<bool>) {
    let color_name: HashMap<&str, &str> = colors
        .iter()
        .map(|(c, n)| (c.as_str(), n.as_str()))
        .collect();
    let size_name: HashMap<&str, &str> = sizes
        .iter()
        .map(|(c, n)| (c.as_str(), n.as_str()))
        .collect();
    let stocks = st.get("stocks");
    let mut per_color: Vec<(String, Vec<(String, bool)>)> = Vec::new();
    let mut any = false;
    let mut seen = false;
    for l2 in st
        .get("l2s")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(c), Some(z), Some(l2id)) = (
            l2.pointer("/color/displayCode").and_then(Value::as_str),
            l2.pointer("/size/displayCode").and_then(Value::as_str),
            l2.get("l2Id").and_then(Value::as_str),
        ) else {
            continue;
        };
        let status = stocks
            .and_then(|s| s.get(l2id))
            .and_then(|s| s.get("statusCode"))
            .and_then(Value::as_str);
        let available = status.is_some_and(|s| s != "STOCK_OUT")
            && l2.get("sales").and_then(Value::as_bool) != Some(false);
        seen |= status.is_some();
        any |= available;
        let cname = color_name.get(c).copied().unwrap_or(c).to_string();
        let zname = size_name.get(z).copied().unwrap_or(z).to_string();
        match per_color
            .iter_mut()
            .find(|(n, _)| *n == format!("{c} {cname}"))
        {
            Some((_, v)) => v.push((zname, available)),
            None => per_color.push((format!("{c} {cname}"), vec![(zname, available)])),
        }
    }
    let variants = per_color
        .into_iter()
        .map(|(c, sizes)| {
            let list: Vec<String> = sizes
                .into_iter()
                .map(|(z, ok)| if ok { z } else { format!("{z} (sold out)") })
                .collect();
            format!("{c}: {}", list.join("/"))
        })
        .collect();
    (variants, seen.then_some(any))
}

fn get_json(fetcher: &Fetcher, url: &str) -> Result<String> {
    let resp = fetcher.get(url, HEADERS)?;
    match resp.status {
        403 | 429 | 503 => Err(FetchError::Blocked(format!("Uniqlo: HTTP {}", resp.status)).into()),
        _ => Ok(resp.body),
    }
}

impl Connector for Uniqlo {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let body = get_json(fetcher, &search_url(query)?)?;
        parse_search(&body, query.page.max(1))
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let (id, group) = parse_id(id_or_url)?;
        let base = format!("{API}/products/{id}/price-groups/{group}");
        let details = get_json(fetcher, &format!("{base}/details?httpFailure=true"))?;
        let l2s = get_json(
            fetcher,
            &format!("{base}/l2s?withPrices=true&withStocks=true&httpFailure=true"),
        )
        .ok();
        parse_item(&details, l2s.as_deref(), &group)
    }
}

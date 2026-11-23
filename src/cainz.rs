use anyhow::{Result, anyhow, bail};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    next::{self, push_unique, text, yen},
    product::{Connector, Product, Query, SearchPage, Sort, unsupported_sort},
};

const SITE: &str = "https://www.cainz.com";

pub struct Cainz;

pub fn sort_param(sort: Sort) -> Result<Option<&'static str>> {
    match sort {
        Sort::Relevance => Ok(None),
        Sort::PriceAsc => Ok(Some("lowPrice")),
        Sort::PriceDesc => Ok(Some("highPrice")),
        Sort::Reviews | Sort::Newest => Err(unsupported_sort("Cainz", sort)),
    }
}

pub fn search_url(query: &Query) -> Result<String> {
    let mut url = url::Url::parse(&format!("{SITE}/search/"))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("q", query.text.trim());
        if let Some(s) = sort_param(query.sort)? {
            q.append_pair("sortRule", s);
        }
        if query.limit.is_some_and(|n| n > 40) {
            q.append_pair("displaySize", "80");
        }
        if query.page > 1 {
            q.append_pair("page", &query.page.to_string());
        }
    }
    Ok(url.into())
}

pub fn product_url(id: &str) -> String {
    format!("{SITE}/g/{id}.html")
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    let candidate = match input.find("/g/") {
        Some(i) => input[i + 3..]
            .split(['/', '?', '#', '.'])
            .next()
            .unwrap_or(""),
        None => input,
    };
    (candidate.len() >= 6 && candidate.chars().all(|c| c.is_ascii_digit()))
        .then(|| candidate.to_string())
}

fn variants(p: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for var in p["variationField"]["variations"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let label = text(&var["label"]);
        for opt in var["options"].as_array().into_iter().flatten() {
            if let Some(name) = text(&opt["name"]) {
                let s = match &label {
                    Some(l) => format!("{l}: {name}"),
                    None => name,
                };
                push_unique(&mut out, s);
            }
        }
    }
    out
}

fn reviews(p: &Value) -> (Option<f64>, Option<u64>) {
    let count = p["review"]["totalReviews"].as_u64();
    (next::rating(&p["review"]["score"], count), count)
}

fn brand(p: &Value) -> Option<String> {
    text(&p["productExplanationField"]["brandName"]).filter(|b| b != "なし")
}

pub fn parse_search(html: &str) -> Result<SearchPage> {
    let flight = next::flight(html);
    let Some(res) = next::values(&flight, "searchResult").find(|v| v["products"].is_array()) else {
        bail!("Cainz: no search results in the page (layout changed?)");
    };
    let items = res["products"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_search_item)
        .collect();
    let page = res["pageNo"]
        .as_u64()
        .or_else(|| res["pageNo"].as_str()?.parse().ok())
        .unwrap_or(1) as u32;
    Ok(SearchPage {
        total: res["totalCount"].as_u64(),
        page,
        items,
    })
}

fn parse_search_item(p: &Value) -> Option<Product> {
    let id = text(&p["productId"])?;
    let base = &p["base"];
    let mut badges = Vec::new();
    if let Some(l) = text(&p["label"]["labelText"]) {
        badges.push(l);
    }
    if base["isOnSale"].as_bool() == Some(true) {
        push_unique(&mut badges, "セール".to_string());
    }
    if let Some(d) = text(&base["description"]) {
        push_unique(&mut badges, d);
    }
    let (rating, rating_count) = reviews(p);
    Some(Product {
        url: product_url(&id),
        name: text(&p["name"])?,
        brand: brand(p),
        price: yen(&base["price"]),
        rating,
        rating_count,
        in_stock: base["selectable"].as_bool().filter(|s| !s),
        badges,
        variants: variants(p),
        image: text(&p["imageUrl"]),
        id,
        ..Product::default()
    })
}

fn availability(ld: &Value) -> Option<bool> {
    let a = ld["offers"]["availability"].as_str()?;
    if a.ends_with("InStock") || a.ends_with("LimitedAvailability") {
        Some(true)
    } else if a.ends_with("OutOfStock") || a.ends_with("SoldOut") || a.ends_with("Discontinued") {
        Some(false)
    } else {
        None
    }
}

pub fn parse_item(html: &str) -> Result<Product> {
    let flight = next::flight(html);
    let lds = next::ld_json(html);
    let ld = next::ld_of_type(&lds, "Product");
    let Some(p) = next::values(&flight, "product")
        .find(|v| v["productId"].is_string() && v["name"].is_string())
    else {
        if ld.is_none() {
            return Err(
                FetchError::NotFound("Cainz product (no product data on the page)".into()).into(),
            );
        }
        bail!("Cainz: product data missing from the page (layout changed?)");
    };
    let p = &p;
    let id = text(&p["productId"]).unwrap_or_default();
    let explanation = &p["productExplanationField"];

    let mut details = Vec::new();
    let mut specs: Vec<&Value> = explanation["specCategories"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    specs.sort_by_key(|s| s["sortOrder"].as_str().unwrap_or("").to_string());
    for s in specs {
        if let (Some(k), Some(v)) = (text(&s["name"]), text(&s["value"])) {
            details.push((k, v));
        }
    }

    let bases: Vec<&Value> = p["bases"].as_array().into_iter().flatten().collect();
    for b in &bases {
        let Some(title) = text(&b["title"]) else {
            continue;
        };
        let inv = &b["inventoryInfo"];
        let mut v = text(&inv["text"]).unwrap_or_else(|| {
            if b["selectable"].as_bool() == Some(false) {
                "不可".to_string()
            } else {
                "-".to_string()
            }
        });
        if let Some(q) = inv["quantityAvailable"].as_u64() {
            v.push_str(&format!(" {q}点"));
        }
        if b["type"].as_str() == Some("pickup")
            && let Some(store) = text(&inv["storeCode"]).or_else(|| text(&b["storeCode"]))
        {
            v.push_str(&format!(" (店舗コード {store})"));
        }
        if let Some(d) = text(&b["description"]) {
            v.push_str(&format!(" {d}"));
        }
        details.push((format!("在庫 {title}"), v));
    }

    let mut badges = Vec::new();
    for l in p["labels"].as_array().into_iter().flatten() {
        if let Some(t) = text(&l["labelText"]) {
            push_unique(&mut badges, t);
        }
    }
    let categories = p["breadcrumbs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| text(&c["name"]))
        .collect();

    let price = bases
        .iter()
        .find(|b| b["type"].as_str() == Some("delivery"))
        .or(bases.first())
        .and_then(|b| yen(&b["price"]))
        .or_else(|| ld.and_then(|l| yen(&l["offers"]["price"])));
    let stocked = bases
        .iter()
        .filter_map(|b| b["inventoryInfo"]["quantityAvailable"].as_u64())
        .max();
    let in_stock = ld.and_then(availability).or(stocked.map(|q| q > 0));
    let description = text(&explanation["overview"])
        .or_else(|| text(&explanation["productCharacteristics"]))
        .or_else(|| ld.and_then(|l| text(&l["description"])));
    let image = text(&p["imageUrls"][0])
        .or_else(|| text(&p["imageUrl"]))
        .or_else(|| ld.and_then(|l| text(&l["image"][0])));
    let (rating, rating_count) = reviews(p);

    Ok(Product {
        url: product_url(&id),
        name: text(&p["name"]).unwrap_or_else(|| id.clone()),
        brand: brand(p).or_else(|| ld.and_then(|l| text(&l["brand"]["name"]))),
        price,
        list_price: None,
        rating,
        rating_count,
        in_stock,
        badges,
        variants: variants(p),
        categories,
        details,
        description,
        image,
        id,
    })
}

fn fetch_page(fetcher: &Fetcher, url: &str, what: &str) -> Result<String> {
    let r = fetcher.get(
        url,
        &[
            (
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
            ("Accept-Language", "ja,en;q=0.8"),
        ],
    )?;
    match r.status {
        404 | 410 => Err(FetchError::NotFound(format!("Cainz {what}")).into()),
        403 | 429 | 503 => {
            Err(FetchError::Blocked(format!("Cainz: HTTP {} on {what}", r.status)).into())
        }
        s if s >= 400 => bail!("Cainz {what} returned HTTP {s}"),
        _ => Ok(r.body),
    }
}

impl Connector for Cainz {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let body = fetch_page(fetcher, &search_url(query)?, "search")?;
        let mut page = parse_search(&body)?;
        page.page = query.page.max(1);
        Ok(page)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let id = parse_id(id_or_url)
            .ok_or_else(|| anyhow!("not a Cainz product id (JAN) or URL: {id_or_url}"))?;
        let body = fetch_page(fetcher, &product_url(&id), &format!("product {id}"))?;
        parse_item(&body)
    }
}

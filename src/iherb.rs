use anyhow::{Result, bail};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    product::{Connector, Product, Query, SearchPage, Sort},
};

const SITE: &str = "https://jp.iherb.com";
const CATALOG: &str = "https://catalog.app.iherb.com";
const IMAGE: &str = "https://cloudinary.images-iherb.com/image/upload/f_auto,q_auto:eco/images";
const COOKIE: &str = "ih-preference=country=JP&currency=JPY&language=ja-JP";
const ATTEMPTS: usize = 8;

pub struct Iherb;

pub fn search_url(q: &Query) -> Result<String> {
    let sr = match q.sort {
        Sort::Relevance => "0",
        Sort::PriceAsc => "4",
        Sort::PriceDesc => "3",
        Sort::Reviews => "12",
        Sort::Newest => "10",
    };
    let mut url = url::Url::parse(&format!("{SITE}/search"))?;
    {
        let mut p = url.query_pairs_mut();
        p.append_pair("kw", q.text);
        p.append_pair("sr", sr);
        if q.page > 1 {
            p.append_pair("p", &q.page.to_string());
        }
    }
    Ok(url.into())
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()) {
        return Some(input.trim_start_matches('0').to_string()).filter(|s| !s.is_empty());
    }
    let url = url::Url::parse(input).ok()?;
    if !url.host_str()?.ends_with("iherb.com") {
        return None;
    }
    let mut segs = url.path_segments()?.filter(|s| !s.is_empty());
    if segs.next()? != "pr" {
        return None;
    }
    segs.next_back()
        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_string)
}

pub fn catalog_url(id: &str) -> String {
    format!("{CATALOG}/product/{id}")
}

pub fn is_challenge(body: &str) -> bool {
    body.contains("<title>Just a moment...</title>")
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn text(e: ElementRef) -> String {
    e.text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn digits(s: &str) -> Option<u64> {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

fn yen(s: &str) -> Option<u64> {
    let whole = s.split('.').next().unwrap_or(s);
    digits(whole)
}

pub fn parse_search(body: &str, page: u32) -> SearchPage {
    let doc = Html::parse_document(body);
    let cell = sel("div.product-cell-container");
    let link = sel("a.product-link");
    let image = sel(".product-image img");
    let price = sel(".product-price .price bdi");
    let old_price = sel(".product-price .price-olp bdi");
    let off = sel(".product-price .percentage-off bdi");
    let flag = sel(".product-flag bdi");
    let promo = sel(".public-pcode-message bdi");
    let stars = sel(".rating a.stars");
    let count = sel(".rating a.rating-count span");

    let mut items = Vec::new();
    for c in doc.select(&cell) {
        let Some(a) = c.select(&link).next() else {
            continue;
        };
        let attr = |k: &str| a.value().attr(k).map(str::to_string);
        let Some(id) = attr("data-product-id").or_else(|| attr("data-ga-product-id")) else {
            continue;
        };
        let first = |s: &Selector| c.select(s).next().map(text).filter(|t| !t.is_empty());
        let out_of_stock = a.value().attr("data-ga-is-out-of-stock") == Some("True")
            || a.value().attr("data-ga-is-discontinued") == Some("True");
        let current = first(&price)
            .and_then(|t| yen(&t))
            .or_else(|| attr("data-ga-discount-price").and_then(|t| yen(&t)));
        let mut badges: Vec<String> = c
            .select(&flag)
            .map(text)
            .filter(|t| !t.is_empty())
            .collect();
        badges.extend(first(&off));
        badges.extend(first(&promo));
        let rating = c
            .select(&stars)
            .next()
            .and_then(|s| s.value().attr("title"))
            .and_then(|t| t.split('/').next())
            .and_then(|t| t.trim().parse::<f64>().ok());
        items.push(Product {
            url: attr("href").unwrap_or_else(|| format!("{SITE}/pr/-/{id}")),
            name: attr("title")
                .map(|t| t.trim().to_string())
                .unwrap_or_default(),
            brand: attr("data-ga-brand-name").filter(|b| !b.is_empty()),
            price: current,
            list_price: first(&old_price)
                .and_then(|t| yen(&t))
                .filter(|l| Some(*l) > current),
            rating,
            rating_count: c.select(&count).next().and_then(|e| digits(&text(e))),
            in_stock: Some(!out_of_stock),
            badges,
            image: c
                .select(&image)
                .next()
                .and_then(|i| i.value().attr("src"))
                .map(str::to_string),
            id,
            ..Product::default()
        });
    }

    let total = doc
        .select(&sel("[data-testid=result-count]"))
        .next()
        .map(text)
        .and_then(|t| {
            let head = t.split("件中").next().filter(|h| h.len() < t.len())?;
            let mut tail: Vec<char> = head
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .collect();
            tail.reverse();
            digits(&tail.into_iter().collect::<String>())
        });
    SearchPage { total, page, items }
}

pub fn html_to_text(html: &str) -> String {
    let marked = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n")
        .replace("</tr>", "\n")
        .replace("</div>", "\n")
        .replace("</li>", "\n")
        .replace("<li>", "・")
        .replace("</td>", " ");
    let frag = Html::parse_fragment(&marked);
    let raw: String = frag.root_element().text().collect();
    raw.replace(['\u{200b}', '\u{feff}'], "")
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn image_url(v: &Value) -> Option<String> {
    let part = str_of(v, "partNumber")?.to_lowercase();
    let prefix = part.split('-').next().filter(|p| !p.is_empty())?;
    let idx = v.get("primaryImageIndex").and_then(Value::as_u64)?;
    Some(format!(
        "{IMAGE}/{prefix}/{}/l/{idx}.jpg",
        part.replace('-', "")
    ))
}

pub fn parse_item(body: &str) -> Result<Product> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("iHerb catalog returned a non-JSON body: {e}"))?;
    let Some(id) = v.get("id").and_then(Value::as_u64) else {
        let msg = str_of(&v, "errorMessage").unwrap_or("response has no product id");
        bail!("iHerb catalog error: {msg}");
    };
    let amount = |k: &str| {
        v.get(k)
            .and_then(Value::as_f64)
            .filter(|a| *a > 0.0)
            .map(|a| a.round() as u64)
    };
    let price = amount("discountPriceAmount").or_else(|| amount("listPriceAmount"));
    let list_price = amount("listPriceAmount").filter(|l| Some(*l) > price);
    let hidden = v.get("hidePrice").and_then(Value::as_bool) == Some(true);
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);

    let mut badges: Vec<String> = v
        .get("flag")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| str_of(f, "displayName"))
        .map(str::to_string)
        .collect();
    if let Some(p) = v
        .get("salesDiscountPercentage")
        .and_then(Value::as_f64)
        .filter(|p| *p > 0.0)
    {
        badges.push(format!("{}％OFF", p.round()));
    }
    if flag("isNew") {
        badges.push("新商品".into());
    }
    if flag("isComingSoon") {
        badges.push("近日入荷".into());
    }
    if flag("isDiscontinued") {
        badges.push("販売終了".into());
    }

    let mut categories: Vec<String> = Vec::new();
    for path in v
        .get("canonicalPaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter(|p| {
            !p.iter()
                .any(|e| str_of(e, "url").is_some_and(|u| u.ends_with("/brands-a-z")))
        })
    {
        for name in path.iter().filter_map(|e| str_of(e, "displayName")) {
            if name != "カテゴリー" && !categories.iter().any(|c| c == name) {
                categories.push(name.to_string());
            }
        }
    }

    let mut details: Vec<(String, String)> = Vec::new();
    let mut add = |k: &str, val: Option<String>| {
        if let Some(val) = val.filter(|s| !s.is_empty()) {
            details.push((k.to_string(), val));
        }
    };
    add("商品コード", str_of(&v, "partNumber").map(str::to_string));
    add("内容量", str_of(&v, "packageQuantity").map(str::to_string));
    add("単価", str_of(&v, "pricePerUnit").map(str::to_string));
    add("重量", str_of(&v, "weight").map(str::to_string));
    add("寸法", str_of(&v, "dimensions").map(str::to_string));
    add(
        "賞味期限",
        str_of(&v, "formattedExpirationDate").map(str::to_string),
    );
    add(
        "発売日",
        str_of(&v, "formattedOnSaleDate").map(str::to_string),
    );
    add("原産国", str_of(&v, "countryOfOrigin").map(str::to_string));
    add(
        "ランキング",
        v.get("productRanks").and_then(Value::as_array).map(|r| {
            r.iter()
                .filter_map(|x| {
                    Some(format!(
                        "{} {}位",
                        str_of(x, "categoryDisplayName")?,
                        str_of(x, "rank")?
                    ))
                })
                .collect::<Vec<_>>()
                .join(", ")
        }),
    );
    for (label, key) in [
        ("成分表示", "supplementFacts"),
        ("成分", "ingredients"),
        ("使用方法", "suggestedUse"),
        ("注意事項", "warnings"),
    ] {
        add(
            label,
            str_of(&v, key).map(|t| html_to_text(t).replace('\n', " / ")),
        );
    }

    let slug = str_of(&v, "urlName");
    let url = str_of(&v, "url")
        .map(str::to_string)
        .or_else(|| slug.map(|s| format!("{SITE}/pr/{s}/{id}")))
        .unwrap_or_else(|| format!("{SITE}/pr/-/{id}"));

    Ok(Product {
        id: id.to_string(),
        name: str_of(&v, "displayName").unwrap_or_default().to_string(),
        url,
        brand: str_of(&v, "brandName").map(str::to_string),
        price: if hidden { None } else { price },
        list_price: if hidden { None } else { list_price },
        rating: v.get("averageRating").and_then(Value::as_f64),
        rating_count: v.get("totalRatingCount").and_then(Value::as_u64),
        in_stock: Some(
            v.get("isAvailableToPurchase").and_then(Value::as_bool) == Some(true)
                && !flag("isDiscontinued"),
        ),
        badges,
        variants: Vec::new(),
        categories,
        details,
        description: str_of(&v, "description")
            .map(html_to_text)
            .filter(|d| !d.is_empty()),
        image: image_url(&v),
    })
}

fn raw_get(fetcher: &Fetcher, url: &str, accept: &str) -> Result<(u16, String)> {
    let r = fetcher.get_raw(
        url,
        &[
            ("Accept", accept),
            ("Accept-Language", "ja-JP,ja;q=0.9"),
            ("Accept-Encoding", "identity"),
            ("Cookie", COOKIE),
        ],
    )?;
    Ok((r.status, r.body))
}

impl Iherb {
    fn get_ok(&self, fetcher: &Fetcher, url: &str, accept: &str) -> Result<(u16, String)> {
        let mut last = String::from("Cloudflare challenge");
        for _ in 0..ATTEMPTS {
            let (status, body) = raw_get(fetcher, url, accept)?;
            if status == 403 || status == 429 || is_challenge(&body) {
                last = format!("HTTP {status} Cloudflare challenge");
                continue;
            }
            return Ok((status, body));
        }
        Err(FetchError::Blocked(format!(
            "iHerb: {last} on {ATTEMPTS} attempts (retry later)"
        ))
        .into())
    }
}

impl Connector for Iherb {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let url = search_url(query)?;
        let (status, body) = self.get_ok(fetcher, &url, "text/html")?;
        if status >= 400 {
            bail!("iHerb search returned HTTP {status}");
        }
        Ok(parse_search(&body, query.page.max(1)))
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let Some(id) = parse_id(id_or_url) else {
            return Err(FetchError::NotFound(format!(
                "'{id_or_url}' is not an iHerb product id or jp.iherb.com/pr/... URL"
            ))
            .into());
        };
        let (status, body) = self.get_ok(fetcher, &catalog_url(&id), "application/json")?;
        match status {
            404 => Err(FetchError::NotFound(format!("iHerb product {id}")).into()),
            400 => Err(FetchError::NotFound(format!("iHerb product {id} (invalid id)")).into()),
            s if s >= 400 => bail!("iHerb catalog returned HTTP {s}"),
            _ => parse_item(&body),
        }
    }
}

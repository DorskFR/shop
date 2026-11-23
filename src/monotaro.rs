use anyhow::{Result, anyhow, bail};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    next::{self, push_unique, text, yen},
    product::{Connector, Product, Query, SearchPage, Sort, unsupported_sort},
};

const SITE: &str = "https://www.monotaro.com";
const IMAGES: &str = "https://jp.images-monotaro.com/Monotaro3/pi/full/";

pub struct Monotaro;

#[derive(Debug, PartialEq, Eq)]
pub enum Id {
    /// Product group page, `/g/<8 digits>/`, the id search returns.
    Group(String),
    /// One orderable item, `/p/<4>/<4>/` (注文コード).
    Item(String),
}

impl Id {
    pub fn url(&self) -> String {
        match self {
            Id::Group(g) => format!("{SITE}/g/{g}/"),
            Id::Item(i) => format!("{SITE}/p/{}/{}/", &i[..4], &i[4..]),
        }
    }
}

pub fn sort_param(sort: Sort) -> Result<Option<&'static str>> {
    match sort {
        Sort::Relevance => Ok(None),
        Sort::PriceAsc => Ok(Some("price")),
        Sort::PriceDesc => Ok(Some("-price")),
        Sort::Reviews => Ok(Some("review_count")),
        Sort::Newest => Err(unsupported_sort("Monotaro", sort)),
    }
}

pub fn search_url(query: &Query) -> Result<String> {
    let mut url = url::Url::parse(&format!("{SITE}/s/"))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("q", query.text.trim());
        if let Some(s) = sort_param(query.sort)? {
            q.append_pair("sort", s);
        }
        if query.page > 1 {
            q.append_pair("page", &query.page.to_string());
        }
    }
    Ok(url.into())
}

fn digits(s: &str, n: usize) -> Option<String> {
    (s.len() == n && s.chars().all(|c| c.is_ascii_digit())).then(|| s.to_string())
}

pub fn parse_id(input: &str) -> Option<Id> {
    let input = input.trim();
    if let Some(i) = input.find("/p/") {
        let mut parts = input[i + 3..].split('/');
        let a = parts.next()?;
        let b = parts.next()?.split(['?', '#']).next()?;
        return digits(&format!("{a}{b}"), 8).map(Id::Item);
    }
    if let Some(i) = input.find("/g/") {
        let g = input[i + 3..].split(['/', '?', '#']).next()?;
        return digits(g, 8).map(Id::Group);
    }
    let bare = input.trim_start_matches(['g', 'G']);
    digits(bare, 8).map(Id::Group)
}

pub fn ship_label(days: u64) -> String {
    match days {
        0 => "当日出荷".to_string(),
        1 => "翌日以内出荷".to_string(),
        n => format!("{n}日以内出荷"),
    }
}

fn part_number(v: &Value) -> Option<String> {
    text(v).filter(|s| s != "-")
}

pub fn parse_search(html: &str) -> Result<SearchPage> {
    let flight = next::flight(html);
    let Some(vm) =
        next::values(&flight, "searchProductsViewModel").find(|v| v["products"].is_array())
    else {
        bail!("Monotaro: no search results in the page (layout changed?)");
    };
    let items = vm["products"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_search_item)
        .collect();
    Ok(SearchPage {
        total: vm["pageInfo"]["totalCount"].as_u64(),
        page: vm["pageInfo"]["page"].as_u64().unwrap_or(1) as u32,
        items,
    })
}

fn parse_search_item(p: &Value) -> Option<Product> {
    let id = text(&p["padProductCode"])?;
    let price = &p["priceInfo"];
    let count = p["reviewInfo"]["reviewCount"].as_u64();
    let ship = &p["shipInfo"];
    let mut badges = Vec::new();
    if let Some(d) = ship["maxShipDays"].as_u64() {
        badges.push(ship_label(d));
    }
    if price["isSpecialPrice"].as_bool() == Some(true) {
        badges.push("特価".to_string());
    }
    let mut details = Vec::new();
    let variants = p["itemCount"].as_u64().unwrap_or(0);
    if variants > 1 {
        details.push(("バリエーション".to_string(), format!("{variants}点")));
    }
    let first = &p["filteredItemInfo"]["filteredItems"][0];
    if let Some(code) = text(&first["padMonotaroNo"]) {
        details.push(("注文コード".to_string(), code));
    }
    if let Some(pn) = part_number(&first["makerProductCode"]) {
        details.push(("品番".to_string(), pn));
    }
    if let Some(q) = text(&p["commonQuantity"]) {
        details.push(("内容量".to_string(), q));
    }
    let name = match text(&p["subName"]) {
        Some(sub) => format!("{} {sub}", text(&p["productName"])?),
        None => text(&p["productName"])?,
    };
    Some(Product {
        url: text(&p["productPageUrl"])
            .map(|u| format!("{SITE}{u}"))
            .unwrap_or_else(|| Id::Group(id.clone()).url()),
        name,
        brand: text(&p["brandInfo"]["name"]),
        price: (price["canDisplay"].as_bool() != Some(false))
            .then(|| yen(&price["taxPrice"]))
            .flatten(),
        rating: next::rating(&p["reviewInfo"]["avgReviewRank"], count),
        rating_count: count,
        in_stock: p["typeInfo"]["canOrder"].as_bool().filter(|o| !o),
        badges,
        details,
        categories: p["mainCategoryPath"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| text(&c["name"]))
            .collect(),
        description: text(&p["feature"]),
        image: text(&p["image"]["representativeImage"]).map(|f| format!("{IMAGES}{f}")),
        id,
        ..Product::default()
    })
}

#[derive(Clone, Copy)]
struct Variant<'a> {
    node: &'a Value,
}

impl Variant<'_> {
    fn code(&self) -> Option<String> {
        text(&self.node["monotaroNo"])
    }
    fn price(&self) -> Option<u64> {
        let p = &self.node["representativePrice"];
        yen(&p["minSalesPrice"]["withTax"]).or_else(|| yen(&p["basePrice"]["withTax"]))
    }
    fn list_price(&self) -> Option<u64> {
        yen(&self.node["representativePrice"]["basePrice"]["withTax"])
            .filter(|lp| self.price().is_some_and(|p| *lp > p))
    }
    fn attrs(&self) -> Vec<(String, String)> {
        let a = &self.node["attribute"];
        let mut out = Vec::new();
        for x in self.node["allAttrs"].as_array().into_iter().flatten() {
            let vals: Vec<String> = x["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                .collect();
            if let (Some(k), false) = (text(&x["name"]), vals.is_empty()) {
                out.push((k, vals.join(", ")));
            }
        }
        let flat = a["attributes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|x| x["canDisplay"].as_bool() != Some(false))
            .chain(
                a["itemAttributes"]["values"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            );
        for x in flat {
            if let (Some(k), Some(v)) = (text(&x["name"]), text(&x["value"]))
                && !v.contains('<')
                && !out.iter().any(|(ok, _)| *ok == k)
            {
                out.push((k, v));
            }
        }
        out
    }
    fn can_order(&self) -> Option<bool> {
        self.node["availability"]["canOrder"].as_bool()
    }
    fn ship(&self) -> Option<String> {
        let a = &self.node["availability"];
        if a["canOrder"].as_bool() == Some(false) {
            return Some("注文不可".to_string());
        }
        a["stdShipDays"].as_u64().map(ship_label)
    }
}

pub fn parse_item(html: &str, id: &Id) -> Result<Product> {
    let flight = next::flight(html);
    let lds = next::ld_json(html);
    let group_ld = next::ld_of_type(&lds, "ProductGroup");
    let product_ld = next::ld_of_type(&lds, "Product");
    let edges = next::values(&flight, "itemConnection")
        .find(|v| v["edges"].is_array())
        .unwrap_or_default();
    let nodes: Vec<Variant> = edges["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| Variant { node: &e["node"] })
        .filter(|v| v.node.is_object())
        .collect();
    if nodes.is_empty() && group_ld.is_none() && product_ld.is_none() {
        return Err(
            FetchError::NotFound("Monotaro product (no product data on the page)".into()).into(),
        );
    }

    let info = match id {
        Id::Item(code) => next::values(&flight, "itemInfo")
            .find(|v| v["monotaroNo"].as_str() == Some(code.as_str())),
        Id::Group(_) => None,
    };
    let selected = match (id, &info) {
        (_, Some(node)) => Some(Variant { node }),
        (Id::Item(code), None) => nodes
            .iter()
            .find(|v| v.code().as_deref() == Some(code.as_str()))
            .copied(),
        (Id::Group(_), None) if nodes.len() == 1 => nodes.first().copied(),
        (Id::Group(_), None) => None,
    };
    let selected = selected.as_ref();
    let variant_ld = group_ld.and_then(|g| {
        g["hasVariant"]
            .as_array()?
            .iter()
            .find(|v| v["sku"].is_string() && v["offers"].is_object())
    });
    let main_ld = variant_ld.or(product_ld).or(group_ld);
    let ld_field = |k: &str| main_ld.map(|l| &l[k]).filter(|v| !v.is_null());

    let reviews = ld_field("aggregateRating").and_then(|r| r["reviewCount"].as_u64());
    let rating = ld_field("aggregateRating").and_then(|r| next::rating(&r["ratingValue"], reviews));

    let mut details = Vec::new();
    let mut variants = Vec::new();
    if let Some(v) = selected {
        let n = v.node;
        if let Some(c) = v.code() {
            details.push(("注文コード".to_string(), c));
        }
        if let Some(pn) = part_number(&n["makerProductCode"]) {
            details.push(("品番".to_string(), pn));
        }
        if let Some(q) = text(&n["quantity"]) {
            details.push(("内容量".to_string(), q));
        }
        if let Some(s) = v.ship() {
            details.push(("出荷目安".to_string(), s));
        }
        details.extend(v.attrs());
    } else {
        for v in &nodes {
            let n = v.node;
            let Some(code) = v.code() else { continue };
            let mut row: Vec<String> = Vec::new();
            if let Some(pn) = part_number(&n["makerProductCode"]) {
                row.push(format!("品番 {pn}"));
            }
            if let Some(s) = text(&n["subName"]) {
                row.push(s.clone());
                push_unique(&mut variants, s);
            }
            if let Some(p) = v.price() {
                row.push(format!("¥{p}"));
            }
            if let Some(s) = v.ship() {
                row.push(s);
            }
            details.push((code, row.join(" | ")));
        }
    }

    let categories = next::ld_of_type(&lds, "BreadcrumbList")
        .and_then(|b| b["itemListElement"].as_array())
        .into_iter()
        .flatten()
        .filter(|e| {
            e["item"]["@id"]
                .as_str()
                .is_some_and(|u| u.contains("/s/c-"))
        })
        .filter_map(|e| text(&e["item"]["name"]))
        .collect();

    let price = match selected {
        Some(v) => v.price(),
        None => nodes.iter().filter_map(Variant::price).min(),
    }
    .or_else(|| ld_field("offers").and_then(|o| yen(&o["price"])));
    let in_stock = match selected {
        Some(v) => v.can_order(),
        None => {
            let flags: Vec<bool> = nodes.iter().filter_map(Variant::can_order).collect();
            (!flags.is_empty()).then(|| flags.iter().any(|f| *f))
        }
    };

    let name = match selected {
        Some(v) => text(&v.node["subName"])
            .and_then(|sub| {
                let base = text(&group_ld?["name"]).or_else(|| ld_field("name").and_then(text))?;
                Some(if base.ends_with(&sub) {
                    base
                } else {
                    format!("{base} {sub}")
                })
            })
            .or_else(|| ld_field("name").and_then(text)),
        None => ld_field("name").and_then(text),
    }
    .unwrap_or_else(|| "?".to_string());
    let image = ld_field("image").and_then(|i| text(i).or_else(|| text(&i[0])));
    let pid = match (id, selected.and_then(Variant::code)) {
        (Id::Item(_), Some(c)) => Id::Item(c),
        (Id::Group(g), _) => Id::Group(g.clone()),
        (Id::Item(c), None) => Id::Item(c.clone()),
    };
    let pid_str = match &pid {
        Id::Group(g) | Id::Item(g) => g.clone(),
    };

    Ok(Product {
        url: pid.url(),
        id: pid_str,
        name,
        brand: ld_field("brand").and_then(|b| text(&b["name"])),
        price,
        list_price: selected.and_then(Variant::list_price),
        rating,
        rating_count: reviews,
        in_stock,
        badges: Vec::new(),
        variants,
        categories,
        details,
        description: ld_field("description").and_then(text),
        image,
    })
}

fn fetch_page(fetcher: &Fetcher, url: &str) -> Result<(u16, String)> {
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
        403 | 429 | 503 => Err(FetchError::Blocked(format!("Monotaro: HTTP {}", r.status)).into()),
        s if s >= 400 && s != 404 => bail!("Monotaro returned HTTP {s} for {url}"),
        s => Ok((s, r.body)),
    }
}

impl Connector for Monotaro {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let (status, body) = fetch_page(fetcher, &search_url(query)?)?;
        if status == 404 {
            return Ok(SearchPage {
                total: Some(0),
                page: query.page.max(1),
                items: Vec::new(),
            });
        }
        let mut page = parse_search(&body)?;
        page.page = query.page.max(1);
        Ok(page)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product> {
        let id = parse_id(id_or_url)
            .ok_or_else(|| anyhow!("not a Monotaro product code or URL: {id_or_url}"))?;
        let (status, body) = fetch_page(fetcher, &id.url())?;
        if status != 404 {
            return parse_item(&body, &id);
        }
        // A bare 8-digit code is either a group code or an order code (注文コード).
        if let Id::Group(code) = &id
            && !id_or_url.contains("/g/")
        {
            let item = Id::Item(code.clone());
            let (status, body) = fetch_page(fetcher, &item.url())?;
            if status != 404 {
                return parse_item(&body, &item);
            }
        }
        Err(FetchError::NotFound(format!("Monotaro product {id_or_url}")).into())
    }
}

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher, Response},
    venue::{Connector, Menu, Query, SearchPage, Sort, Venue, unsupported_sort},
};

const SITE: &str = "https://www.tablecheck.com";
const SEARCH_API: &str = "https://search-v4.production.ap-northeast-1.aws.tablecheck.net";
const SEARCH_KEY: &str = "13ca76faf721de2afbfacd33f35adb8c7f37ac569259a76cd15da066739dedb4";
const PAGE_SIZE: u32 = 20;
const MAX_OFFSET: u32 = 1000;
const LOOKUP_MAX_HITS: u32 = 20;
const PARTY_SIZE: &str = "2";
const LOCALE: &str = "ja";
const PAGE_HEADERS: &[(&str, &str)] = &[
    (
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    ),
    ("Accept-Language", "ja-JP,ja;q=0.9"),
];
const API_HEADERS: &[(&str, &str)] = &[
    ("Accept", "application/json"),
    ("Accept-Language", "ja-JP,ja;q=0.9"),
    ("X-API-Key", SEARCH_KEY),
    ("Origin", SITE),
    ("Referer", "https://www.tablecheck.com/"),
];
const RESERVED: &[&str] = &[
    "japan",
    "account",
    "not-found",
    "shops",
    "search",
    "lists",
    "storefronts",
];

pub struct TableCheck;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Any,
    Cuisines(Vec<String>),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub id: String,
    pub name: String,
}

impl Connector for TableCheck {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        if query.sort != Sort::Relevance {
            return Err(unsupported_sort("TableCheck", query.sort));
        }
        let catalog = fetch_api(fetcher, &catalog_url())
            .and_then(|b| serde_json::from_str::<Value>(&b).context("TableCheck: bad catalog"))?;
        let destination = match query.area.map(str::trim).filter(|a| !a.is_empty()) {
            Some(area) => {
                let body = fetch_api(fetcher, &destination_url(area)?)?;
                Some(
                    parse_destination(&body)?
                        .ok_or_else(|| anyhow!("TableCheck: unknown area {area:?}"))?,
                )
            }
            None => None,
        };
        let keyword = resolve_keyword(query.text, &catalog);
        let url = search_url(query, &keyword, destination.as_ref().map(|d| d.id.as_str()))?;
        let body = fetch_api(fetcher, &url)?;
        let mut page = parse_search_with(&body, Some(&catalog))?;
        page.page = query.page.max(1);
        if matches!(keyword, Keyword::Name(_)) {
            let skip = (page.page - 1) * page_size(query);
            page.items.drain(..(skip as usize).min(page.items.len()));
        }
        Ok(page)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Venue> {
        let slug = parse_id(id_or_url)
            .ok_or_else(|| anyhow!("not a TableCheck shop slug or URL: {id_or_url}"))?;
        let url = item_url(&slug);
        let resp = fetcher.get(&url, PAGE_HEADERS)?;
        check(&resp, &url)?;
        let catalog = fetch_api(fetcher, &catalog_url())
            .ok()
            .and_then(|b| serde_json::from_str::<Value>(&b).ok());
        parse_item(&resp.body, catalog.as_ref()).map_err(|e| match e.downcast_ref() {
            Some(FetchError::NotFound(_)) => {
                FetchError::NotFound(format!("TableCheck shop {url}")).into()
            }
            _ => e,
        })
    }
}

fn check(resp: &Response, url: &str) -> Result<()> {
    match resp.status {
        404 | 410 => Err(FetchError::NotFound(url.to_string()).into()),
        401 | 403 | 429 => {
            Err(FetchError::Blocked(format!("TableCheck: HTTP {}", resp.status)).into())
        }
        s if s >= 500 && is_challenge(&resp.body) => {
            Err(FetchError::Blocked(format!("TableCheck: HTTP {s} challenge")).into())
        }
        s if s >= 400 => bail!("TableCheck: HTTP {s} for {url}"),
        _ if is_challenge(&resp.body) => {
            Err(FetchError::Blocked("TableCheck: bot challenge page".into()).into())
        }
        _ => Ok(()),
    }
}

pub fn is_challenge(body: &str) -> bool {
    let head = &body[..body.floor_char_boundary(4096)];
    [
        "cf-chl",
        "challenge-platform",
        "Just a moment...",
        "captcha",
    ]
    .iter()
    .any(|m| head.contains(m))
}

fn fetch_api(fetcher: &Fetcher, url: &str) -> Result<String> {
    let resp = fetcher.get(url, API_HEADERS)?;
    match resp.status {
        401 | 403 | 429 => Err(FetchError::Blocked(format!(
            "TableCheck search API: HTTP {} (the web app's key may have rotated)",
            resp.status
        ))
        .into()),
        s if s >= 500 => bail!("TableCheck search API: HTTP {s}"),
        _ if resp.body.trim_start().starts_with('<') => Err(FetchError::Blocked(
            "TableCheck search API: got an HTML page instead of JSON".into(),
        )
        .into()),
        _ => Ok(resp.body),
    }
}

pub fn catalog_url() -> String {
    format!("{SEARCH_API}/catalog.json")
}

pub fn shop_url(slug: &str) -> String {
    format!("{SITE}/{LOCALE}/{slug}")
}

pub fn item_url(slug: &str) -> String {
    shop_url(slug)
}

pub fn booking_url(slug: &str) -> String {
    format!("{SITE}/{LOCALE}/shops/{slug}/reserve")
}

fn api_url(pairs: &[(&str, String)]) -> Result<String> {
    let mut url = url::Url::parse(&format!("{SEARCH_API}/search/"))?;
    url.query_pairs_mut()
        .extend_pairs(pairs.iter().map(|(k, v)| (*k, v.as_str())));
    Ok(url.into())
}

pub fn destination_url(area: &str) -> Result<String> {
    api_url(&[
        ("queryProfile", "destination-suggest".into()),
        ("q", area.trim().into()),
        ("country", "JP".into()),
        ("hits", "1".into()),
    ])
}

fn page_size(query: &Query) -> u32 {
    query.limit.unwrap_or(PAGE_SIZE).clamp(1, PAGE_SIZE)
}

pub fn search_url(query: &Query, keyword: &Keyword, destination: Option<&str>) -> Result<String> {
    if query.sort != Sort::Relevance {
        return Err(unsupported_sort("TableCheck", query.sort));
    }
    let size = page_size(query);
    let page = query.page.max(1);
    let destination = destination.unwrap_or("country:jp").to_string();
    if let Keyword::Name(name) = keyword {
        let hits = page * size;
        if hits > LOOKUP_MAX_HITS {
            bail!(
                "TableCheck name search returns at most {LOOKUP_MAX_HITS} results; use a smaller --page or -n"
            );
        }
        return api_url(&[
            ("queryProfile", "venue-lookup".into()),
            ("name", name.clone()),
            ("destination", destination),
            ("hits", hits.to_string()),
        ]);
    }
    let offset = (page - 1) * size;
    if offset > MAX_OFFSET {
        bail!("TableCheck search stops after {MAX_OFFSET} results");
    }
    let cuisine = match keyword {
        Keyword::Cuisines(c) if !c.is_empty() => c.join(" "),
        _ => "any".to_string(),
    };
    api_url(&[
        ("queryProfile", "venue-search".into()),
        ("destination", destination),
        ("cuisine", cuisine),
        ("party_size", PARTY_SIZE.into()),
        ("sort", "recommended".into()),
        ("hits", size.to_string()),
        ("offset", offset.to_string()),
    ])
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase().replace('-', "_")
}

fn cuisine_key(word: &str, catalog: &Value) -> Option<String> {
    let w = norm(word);
    let cuisines = catalog["values"]["cuisine"].as_object()?;
    if cuisines.contains_key(&w) {
        return Some(w);
    }
    cuisines.iter().find_map(|(key, c)| {
        let labels = c["label"].as_object().into_iter().flat_map(|m| m.values());
        let aliases = c["aliases"].as_array().into_iter().flatten();
        labels
            .chain(aliases)
            .chain(std::iter::once(&c["reading"]))
            .filter_map(Value::as_str)
            .any(|l| norm(l) == w)
            .then(|| key.clone())
    })
}

pub fn resolve_keyword(text: &str, catalog: &Value) -> Keyword {
    let text = text.trim();
    if text.is_empty() {
        return Keyword::Any;
    }
    if let Some(k) = cuisine_key(text, catalog) {
        return Keyword::Cuisines(vec![k]);
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let keys: Option<Vec<String>> = words.iter().map(|w| cuisine_key(w, catalog)).collect();
    match keys {
        Some(mut keys) if !keys.is_empty() => {
            keys.dedup();
            Keyword::Cuisines(keys)
        }
        _ => Keyword::Name(text.to_string()),
    }
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    let candidate = if input.contains("tablecheck.com") || input.starts_with('/') {
        let path = match input.find("tablecheck.com") {
            Some(i) => &input[i + "tablecheck.com".len()..],
            None => input,
        };
        let path = path.split(['?', '#']).next().unwrap_or("");
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let rest = match segs.first() {
            Some(s) if is_locale(s) => &segs[1..],
            _ => &segs[..],
        };
        match rest {
            ["shops", slug, ..] => *slug,
            [slug, ..] => *slug,
            [] => return None,
        }
    } else {
        input
    };
    let valid = !candidate.is_empty()
        && !RESERVED.contains(&candidate)
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    valid.then(|| candidate.to_string())
}

fn is_locale(s: &str) -> bool {
    let (lang, region) = s.split_once('-').unwrap_or((s, ""));
    lang.len() == 2
        && lang.chars().all(|c| c.is_ascii_lowercase())
        && (region.is_empty()
            || (region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic())))
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(|s| s.replace("\r\n", "\n").trim().to_string())
        .filter(|s| !s.is_empty())
}

fn localized(v: &Value) -> Option<String> {
    if let Some(list) = v.as_array() {
        let pick = |loc: &str| {
            list.iter()
                .find(|t| t["locale"] == loc)
                .and_then(|t| text(&t["translation"]))
        };
        return pick(LOCALE)
            .or_else(|| pick("en"))
            .or_else(|| list.iter().find_map(|t| text(&t["translation"])));
    }
    let map = v.as_object()?;
    text(&v[LOCALE])
        .or_else(|| text(&v["en"]))
        .or_else(|| map.values().find_map(text))
}

fn num(v: &Value) -> Option<u64> {
    let n = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))?;
    (n > 0.0).then(|| n.round() as u64)
}

pub fn yen(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::from("￥");
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn label(catalog: Option<&Value>, kind: &str, key: &str) -> String {
    catalog
        .and_then(|c| localized(&c["values"][kind][key]["label"]))
        .unwrap_or_else(|| key.to_string())
}

fn keys(v: &Value) -> impl Iterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}

fn labels<'a>(
    catalog: Option<&Value>,
    kind: &str,
    keys: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let l = label(catalog, kind, key);
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

fn station_name(name: String) -> String {
    if name.ends_with('駅') {
        name
    } else {
        format!("{name}駅")
    }
}

fn api_root(body: &str, what: &str) -> Result<Value> {
    let v: Value = serde_json::from_str(body)
        .with_context(|| format!("TableCheck: {what} response is not JSON"))?;
    if let Some(e) = text(&v["error"]) {
        bail!("TableCheck: {what} failed: {e}");
    }
    let root = v
        .get("root")
        .ok_or_else(|| anyhow!("TableCheck: unrecognised {what} response (no root)"))?;
    if let Some(e) = root["errors"].as_array().and_then(|a| a.first()) {
        let msg = text(&e["message"])
            .or_else(|| text(&e["summary"]))
            .unwrap_or_else(|| "unknown error".into());
        bail!("TableCheck: {what} failed: {msg}");
    }
    Ok(root.clone())
}

fn hits<'a>(root: &'a Value, doc: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    root["children"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| !c["id"].as_str().unwrap_or("").starts_with("group:"))
        .map(|c| &c["fields"])
        .filter(move |f| f["sddocname"] == doc)
}

pub fn parse_destination(body: &str) -> Result<Option<Destination>> {
    let root = api_root(body, "area lookup")?;
    Ok(hits(&root, "destination").find_map(|f| {
        Some(Destination {
            id: text(&f["id"])?,
            name: localized(&f["names"])?,
        })
    }))
}

pub fn parse_search(body: &str) -> Result<SearchPage> {
    parse_search_with(body, None)
}

pub fn parse_search_with(body: &str, catalog: Option<&Value>) -> Result<SearchPage> {
    let root = api_root(body, "search")?;
    if root["fields"]["totalCount"].is_null() && root["children"].is_null() {
        bail!("TableCheck: unrecognised search response");
    }
    let items = hits(&root, "venue")
        .filter_map(|f| parse_hit(f, catalog))
        .collect();
    Ok(SearchPage {
        total: root["fields"]["totalCount"].as_u64(),
        page: 1,
        items,
    })
}

fn search_budget(f: &Value) -> Option<String> {
    let parts: Vec<String> = [
        ("ランチ", "budget_lunch_avg"),
        ("ディナー", "budget_dinner_avg"),
    ]
    .into_iter()
    .filter_map(|(l, k)| num(&f[k]).map(|n| format!("{l} {}", yen(n))))
    .collect();
    (!parts.is_empty()).then(|| parts.join(" / "))
}

fn parse_hit(f: &Value, catalog: Option<&Value>) -> Option<Venue> {
    let slug = text(&f["slug"])?;
    let name = localized(&f["names"]).or_else(|| text(&f["name"]))?;
    let st = &f["nearest_station"];
    let access = localized(&st["names"]).map(|n| {
        let mut s = station_name(n);
        if let Some(w) = num(&st["walk_minutes"]) {
            s.push_str(&format!(" 徒歩{w}分"));
        }
        if let Some(d) = num(&st["distance_m"]) {
            s.push_str(&format!(" ({d}m)"));
        }
        s
    });
    let address = localized(&f["address"]).map(|a| a.replace(", ", ""));
    Some(Venue {
        url: shop_url(&slug),
        booking_url: Some(booking_url(&slug)),
        name,
        genre: labels(catalog, "cuisine", keys(&f["cuisines"])),
        area: localized(&f["area"]["names"]),
        address,
        access,
        budget: search_budget(f),
        description: localized(&f["headlines"]),
        image: text(&f["image"]).or_else(|| text(&f["photos"][0])),
        id: slug,
        ..Venue::default()
    })
}

fn query_state(html: &str) -> Result<Value> {
    const MARK: &str = "__REACT_QUERY_STATE__=";
    let start = html
        .find(MARK)
        .ok_or_else(|| anyhow!("TableCheck: unrecognised shop page (no embedded query state)"))?;
    serde_json::Deserializer::from_str(&html[start + MARK.len()..])
        .into_iter::<Value>()
        .next()
        .ok_or_else(|| anyhow!("TableCheck: empty query state"))?
        .context("TableCheck: malformed query state")
}

fn query_data<'a>(state: &'a Value, key: &str) -> Option<&'a Value> {
    state["queries"]
        .as_array()?
        .iter()
        .find(|q| q["queryKey"][0] == key)
        .map(|q| &q["state"]["data"])
}

fn range(lo: Option<u64>, hi: Option<u64>) -> Option<String> {
    match (lo, hi) {
        (Some(a), Some(b)) if a != b => Some(format!("{}～{}", yen(a), yen(b))),
        (Some(a), _) | (None, Some(a)) => Some(yen(a)),
        _ => None,
    }
}

fn item_budget(s: &Value) -> Option<String> {
    let parts: Vec<String> = [("ランチ", "lunch"), ("ディナー", "dinner")]
        .into_iter()
        .filter_map(|(l, meal)| {
            range(
                num(&s[format!("budget_{meal}_min")]),
                num(&s[format!("budget_{meal}_max")]),
            )
            .or_else(|| num(&s[format!("budget_{meal}_avg")]).map(yen))
            .map(|r| format!("{l} {r}"))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" / "))
}

fn item_address(a: &Value) -> Option<String> {
    let body: String = ["region", "city", "street"]
        .iter()
        .filter_map(|k| text(&a[*k]))
        .collect();
    if body.is_empty() {
        return None;
    }
    let mut out = match text(&a["postal_code"]) {
        Some(p) => format!("〒{p} {body}"),
        None => body,
    };
    if let Some(s2) = text(&a["street2"]) {
        out.push(' ');
        out.push_str(&s2);
    }
    Some(out)
}

fn item_access(stations: &Value) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut parts = Vec::new();
    for st in stations.as_array().into_iter().flatten() {
        let Some(name) = localized(&st["name_translations"]).map(station_name) else {
            continue;
        };
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        parts.push(match num(&st["distance"]) {
            Some(d) => format!("{name} ({d}m)"),
            None => name,
        });
        if parts.len() == 3 {
            break;
        }
    }
    (!parts.is_empty()).then(|| parts.join(" / "))
}

const DAYS: [(&str, &str); 7] = [
    ("monday", "月"),
    ("tuesday", "火"),
    ("wednesday", "水"),
    ("thursday", "木"),
    ("friday", "金"),
    ("saturday", "土"),
    ("sunday", "日"),
];

fn operating_days(days: &Value) -> (Option<String>, Option<String>) {
    if !days.is_object() {
        return (None, None);
    }
    let mut hours = Vec::new();
    let mut closed = Vec::new();
    for (key, jp) in DAYS {
        let day = &days[key];
        let slots: Vec<String> = day["timeslots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| {
                let mut s = format!(
                    "{}-{}",
                    text(&t["opening_time"])?,
                    text(&t["closing_time"])?
                );
                if let Some(lo) = text(&t["food_lo"]) {
                    s.push_str(&format!(" (LO {lo})"));
                }
                Some(s)
            })
            .collect();
        if day == "closed" || (day.is_object() && slots.is_empty()) {
            closed.push(jp);
        } else if !slots.is_empty() {
            hours.push(format!("{jp} {}", slots.join(", ")));
        }
    }
    (
        (!hours.is_empty()).then(|| hours.join("\n")),
        (!closed.is_empty()).then(|| closed.join("、")),
    )
}

fn parse_menu(m: &Value) -> Option<Menu> {
    if m["is_hidden"] == true || m["is_active"] == false {
        return None;
    }
    let name = localized(&m["name_translations"])?;
    let price = num(&m["price"]);
    let tax = match m["tax_type"].as_str() {
        Some("included") => "（税込）",
        Some("excluded") => "（税抜）",
        _ => "",
    };
    Some(Menu {
        name,
        price,
        price_text: price.map(|p| format!("{}{tax}", yen(p))),
        description: localized(&m["tagline_translations"])
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")),
    })
}

pub fn parse_item(html: &str, catalog: Option<&Value>) -> Result<Venue> {
    let state = query_state(html)?;
    let shop = query_data(&state, "venue")
        .and_then(|d| d["shops"].as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| FetchError::NotFound("TableCheck shop".into()))?;
    let slug = text(&shop["slug"])
        .ok_or_else(|| anyhow!("TableCheck: unrecognised shop data (no slug)"))?;
    let name = localized(&shop["name_translations"])
        .or_else(|| text(&shop["name"][0]))
        .unwrap_or_else(|| slug.clone());

    let menus: Vec<Menu> = query_data(&state, "menuItems")
        .and_then(|d| d["menu_items"].as_array())
        .into_iter()
        .flatten()
        .filter_map(parse_menu)
        .collect();

    let mut details = Vec::new();
    if let Some(k) = text(&shop["kana_name"]) {
        details.push(("読み".to_string(), k));
    }
    let seats: Vec<String> = shop["service_categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| localized(&c["name_translations"]))
        .collect();
    if !seats.is_empty() {
        details.push(("席".to_string(), seats.join("、")));
    }
    let known = |k: &&str| catalog.is_none_or(|c| !c["values"]["feature"][*k].is_null());
    let features = labels(catalog, "feature", keys(&shop["tags"]).filter(known));
    if !features.is_empty() {
        details.push(("特徴".to_string(), features.join("、")));
    }
    if let Some(u) = text(&shop["url"]) {
        details.push(("ウェブサイト".to_string(), u));
    }

    let description = [
        localized(&shop["content_title_translations"]),
        localized(&shop["content_body_translations"]),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n");
    let (hours, closed) = operating_days(&shop["operating_days"]);

    Ok(Venue {
        url: shop_url(&slug),
        booking_url: Some(booking_url(&slug)),
        name,
        genre: labels(catalog, "cuisine", keys(&shop["cuisines"])),
        area: localized(&shop["location_name_translations"])
            .or_else(|| text(&shop["address"]["city"])),
        address: item_address(&shop["address"]),
        access: item_access(&shop["stations"]),
        budget: item_budget(shop),
        hours,
        closed,
        phone: text(&shop["phone_natl"]).or_else(|| text(&shop["phone"])),
        details,
        menus,
        description: (!description.is_empty()).then_some(description),
        image: text(&shop["banner_image"])
            .or_else(|| text(&shop["search_image"]))
            .or_else(|| text(&shop["images"][0]["original"])),
        id: slug,
        ..Venue::default()
    })
}

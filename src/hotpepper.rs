use anyhow::{Result, anyhow, bail};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher, Response},
    venue::{Connector, Menu, Query, SearchPage, Sort, Venue, unsupported_sort},
};

const SITE: &str = "https://beauty.hotpepper.jp";
const HEADERS: &[(&str, &str)] = &[
    (
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    ),
    ("Accept-Language", "ja-JP,ja;q=0.9"),
];

pub struct HotPepper;

impl Connector for HotPepper {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let url = search_url(query)?;
        let resp = fetcher.get(&url, HEADERS)?;
        check(&resp, &url)?;
        parse_search(&resp.body)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Venue> {
        let id = parse_id(id_or_url)
            .ok_or_else(|| anyhow!("not a HotPepper Beauty salon id or URL: {id_or_url}"))?;
        let url = salon_url(&id);
        let resp = fetcher.get(&url, HEADERS)?;
        check(&resp, &url)?;
        let coupon_url = format!("{url}coupon/");
        let coupons = fetcher.get(&coupon_url, HEADERS)?;
        let coupons = match check(&coupons, &coupon_url) {
            Ok(()) => Some(coupons.body),
            Err(e) if matches!(e.downcast_ref(), Some(FetchError::NotFound(_))) => None,
            Err(e) => return Err(e),
        };
        parse_item(&resp.body, coupons.as_deref())
    }
}

pub fn search_url(q: &Query) -> Result<String> {
    if q.sort != Sort::Relevance {
        return Err(unsupported_sort("HotPepper Beauty", q.sort));
    }
    let words: Vec<&str> = [Some(q.text), q.area]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        bail!("missing search query or --area");
    }
    let mut url = url::Url::parse(&format!("{SITE}/CSP/bt/salonSearch/search/"))?;
    {
        let mut p = url.query_pairs_mut();
        p.append_pair("freeword", &words.join(" "));
        p.append_pair("searchGender", "ALL");
        p.append_pair("sortType", "popular");
        if q.page > 1 {
            p.append_pair("pn", &q.page.to_string());
        }
    }
    Ok(url.into())
}

pub fn salon_url(id: &str) -> String {
    format!("{SITE}/sln{id}/")
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    let bare = input.strip_prefix("sln").unwrap_or(input);
    if is_id(bare) {
        return Some(bare.to_ascii_uppercase());
    }
    let url = url::Url::parse(input).ok()?;
    if !url.host_str()?.ends_with("hotpepper.jp") {
        return None;
    }
    let from_path = url
        .path_segments()?
        .filter_map(|s| s.strip_prefix("sln"))
        .find(|s| is_id(s))
        .map(str::to_string);
    from_path
        .or_else(|| {
            url.query_pairs()
                .find(|(k, v)| k == "storeId" && is_id(v))
                .map(|(_, v)| v.into_owned())
        })
        .map(|s| s.to_ascii_uppercase())
}

fn is_id(s: &str) -> bool {
    s.len() > 1 && s.starts_with(['H', 'h']) && s[1..].bytes().all(|b| b.is_ascii_digit())
}

pub fn is_blocked(body: &str) -> Option<&'static str> {
    let title = body
        .split_once("<title")
        .and_then(|(_, t)| t.split_once("</title>"))
        .map_or("", |(t, _)| t)
        .to_lowercase();
    if title.contains("access denied") {
        Some("Akamai (Access Denied)")
    } else if title.contains("just a moment") || body.contains("/cdn-cgi/challenge-platform/") {
        Some("a Cloudflare challenge")
    } else if title.contains("captcha") || title.contains("ロボット") {
        Some("a captcha page")
    } else {
        None
    }
}

fn is_missing(body: &str) -> bool {
    body.contains("<title>掲載エラー") || body.contains("掲載情報がありませんでした")
}

fn check(resp: &Response, url: &str) -> Result<()> {
    if let Some(why) = is_blocked(&resp.body) {
        return Err(FetchError::Blocked(format!("HotPepper Beauty: {why}")).into());
    }
    if resp.status == 404 || is_missing(&resp.body) {
        return Err(FetchError::NotFound(url.to_string()).into());
    }
    if matches!(resp.status, 403 | 429 | 503) {
        return Err(FetchError::Blocked(format!("HotPepper Beauty: HTTP {}", resp.status)).into());
    }
    if resp.status >= 400 {
        bail!("HotPepper Beauty returned HTTP {} for {url}", resp.status);
    }
    Ok(())
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn text(e: ElementRef) -> String {
    squash(&e.text().collect::<String>())
}

fn squash(s: &str) -> String {
    s.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tidy(s: &str) -> String {
    let s = squash(s);
    if s.ends_with([']', '］'])
        && let Some(i) = s.rfind(['[', '［'])
        && i > 0
        && s[i..].contains('/')
    {
        return s[..i].trim_end().to_string();
    }
    s
}

fn digits(s: &str) -> Option<u64> {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

fn yen(s: &str) -> Option<u64> {
    let s = s.trim();
    let start = s.find(['¥', '￥'])?;
    let num: String = s[start..]
        .chars()
        .skip(1)
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(char::is_ascii_digit)
        .collect();
    num.parse().ok()
}

fn genre(name: &str, extra: &str) -> Vec<String> {
    let all = format!("{name} {extra}").to_lowercase();
    let mut g = Vec::new();
    if ["理容", "理髪", "床屋", "バーバー", "barber"]
        .iter()
        .any(|k| all.contains(k))
    {
        g.push("バーバー".to_string());
    }
    let lower = name.to_lowercase();
    if lower.contains("メンズ") || lower.contains("men's") || lower.contains("men’s") {
        g.push("メンズサロン".to_string());
    }
    if g.is_empty() {
        g.push("ヘアサロン".to_string());
    }
    g
}

fn strip_query(href: &str) -> String {
    href.split(['?', '#']).next().unwrap_or(href).to_string()
}

fn absolute(href: &str) -> String {
    if href.starts_with('/') {
        format!("{SITE}{href}")
    } else {
        href.to_string()
    }
}

pub fn parse_search(body: &str) -> Result<SearchPage> {
    if let Some(why) = is_blocked(body) {
        return Err(FetchError::Blocked(format!("HotPepper Beauty: {why}")).into());
    }
    let doc = Html::parse_document(body);
    let total = doc
        .select(&sel("span.numberOfResult"))
        .next()
        .and_then(|e| digits(&text(e)));
    let cassettes: Vec<ElementRef> = doc.select(&sel("ul.slnCassetteList > li")).collect();
    let zero = doc.select(&sel(".searchResultZeroText")).next().is_some();
    if cassettes.is_empty() {
        if zero || total == Some(0) {
            return Ok(SearchPage {
                total: Some(0),
                page: 1,
                items: Vec::new(),
            });
        }
        bail!("HotPepper Beauty search page layout not recognised");
    }

    let name_link = sel("h3.slnName a");
    let catch = sel("p.slnCatch");
    let img = sel("ul.slnImgList img");
    let dd = |c: &str| sel(&format!("dl.slnDetail dd.{c}"));
    let (access, price, seat, message) = (dd("access"), dd("price"), dd("seat"), dd("message"));
    let reserve = sel("a[href*='/CSP/bt/reserve/']");
    let coupon = sel("ul.slnCouponList > li");
    let coupon_name = sel("a.slnCouponLink");
    let coupon_price = sel("p.slnCouponPrice");
    let coupon_label = sel("ul.slnCouponLabelList li");

    let mut items = Vec::new();
    for c in cassettes {
        let Some(a) = c.select(&name_link).next() else {
            continue;
        };
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(id) = parse_id(href) else {
            continue;
        };
        let first = |s: &Selector| c.select(s).next().map(text).filter(|t| !t.is_empty());
        let name = text(a);
        let budget = first(&price).map(|p| format!("カット {p}"));
        let mut details = Vec::new();
        if let Some(s) = first(&seat) {
            details.push(("席数".to_string(), s));
        }
        let menus = c
            .select(&coupon)
            .filter_map(|li| {
                let name = li.select(&coupon_name).next().map(text)?;
                let price_text = li.select(&coupon_price).next().map(text);
                let labels: Vec<String> = li.select(&coupon_label).map(text).collect();
                Some(Menu {
                    name,
                    price: price_text.as_deref().and_then(yen),
                    price_text,
                    description: (!labels.is_empty()).then(|| format!("[{}]", labels.join("・"))),
                })
            })
            .collect();
        items.push(Venue {
            url: strip_query(href),
            genre: genre(&name, ""),
            access: first(&access),
            rating_count: first(&message).and_then(|m| digits(&m)),
            budget,
            booking_url: c
                .select(&reserve)
                .next()
                .and_then(|r| r.value().attr("href"))
                .map(|h| absolute(&strip_query_keep_store(h))),
            details,
            menus,
            description: first(&catch),
            image: c
                .select(&img)
                .next()
                .and_then(|i| i.value().attr("src"))
                .map(str::to_string),
            id,
            name,
            ..Venue::default()
        });
    }
    if items.is_empty() {
        bail!("HotPepper Beauty search page layout not recognised: no salon links");
    }
    Ok(SearchPage {
        total,
        page: current_page(&doc).unwrap_or(1),
        items,
    })
}

fn current_page(doc: &Html) -> Option<u32> {
    let head = doc.select(&sel(".preListHead")).next().map(text)?;
    let (before, _) = head.split_once("ページ")?;
    let (cur, _) = before.rsplit(' ').next()?.split_once('/')?;
    cur.trim().parse().ok()
}

fn strip_query_keep_store(href: &str) -> String {
    let base = strip_query(href);
    match url::Url::parse(&absolute(href)).ok().and_then(|u| {
        u.query_pairs()
            .find(|(k, _)| k == "storeId")
            .map(|(_, v)| v.into_owned())
    }) {
        Some(id) => format!("{base}?storeId={id}"),
        None => base,
    }
}

fn json_ld(doc: &Html) -> Vec<Value> {
    let mut out = Vec::new();
    for s in doc.select(&sel("script[type='application/ld+json']")) {
        let raw: String = s.text().collect();
        match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Array(a)) => out.extend(a),
            Ok(v) => out.push(v),
            Err(_) => {}
        }
    }
    out
}

fn ld_type<'a>(lds: &'a [Value], kind: &str) -> Option<&'a Value> {
    lds.iter()
        .find(|v| v.get("@type").and_then(Value::as_str) == Some(kind))
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn breadcrumb_area(lds: &[Value]) -> Option<String> {
    let list = ld_type(lds, "BreadcrumbList")?
        .get("itemListElement")?
        .as_array()?;
    let parent = list.iter().rev().nth(1)?;
    let name = str_of(parent.get("item")?, "name")?;
    let area = name.strip_suffix("トップ").unwrap_or(name).trim();
    Some(area.to_string()).filter(|a| !a.is_empty() && !a.contains("検索"))
}

pub fn parse_item(body: &str, coupons: Option<&str>) -> Result<Venue> {
    if let Some(why) = is_blocked(body) {
        return Err(FetchError::Blocked(format!("HotPepper Beauty: {why}")).into());
    }
    if is_missing(body) {
        return Err(FetchError::NotFound("HotPepper Beauty salon".into()).into());
    }
    let doc = Html::parse_document(body);
    let lds = json_ld(&doc);
    let salon = ld_type(&lds, "HairSalon");
    let first = |css: &str| {
        doc.select(&sel(css))
            .next()
            .map(text)
            .filter(|t| !t.is_empty())
    };
    let Some(name) = first("p.detailTitle")
        .or_else(|| salon.and_then(|s| str_of(s, "name")).map(str::to_string))
        .or_else(|| first("h1#headSummary"))
    else {
        bail!("HotPepper Beauty salon page layout not recognised");
    };

    let mut table: Vec<(String, String)> = Vec::new();
    for row in doc.select(&sel("table.slnDataTbl tr")) {
        let cells: Vec<ElementRef> = row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|e| matches!(e.value().name(), "th" | "td"))
            .collect();
        for pair in cells.chunks(2) {
            if let [th, td] = pair
                && th.value().name() == "th"
            {
                let v = tidy(&text(*td));
                if !v.is_empty() {
                    table.push((text(*th), v));
                }
            }
        }
    }
    let field = |k: &str| {
        table
            .iter()
            .find(|(n, _)| n.starts_with(k))
            .map(|(_, v)| v.clone())
    };
    let phone = salon
        .and_then(|s| str_of(s, "telephone"))
        .map(str::to_string)
        .or_else(|| field("電話番号").filter(|p| p.chars().any(|c| c.is_ascii_digit())));
    let rating = salon
        .and_then(|s| s.get("aggregateRating"))
        .and_then(|r| r.get("ratingValue"))
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .or_else(|| first(".slnHeaderKuchikomiPoint").and_then(|t| t.parse().ok()))
        .or_else(|| first("dl.rating .rating-score").and_then(|t| t.parse().ok()));
    let rating_count = salon
        .and_then(|s| s.get("aggregateRating"))
        .and_then(|r| r.get("reviewCount"))
        .and_then(|v| v.as_u64().or_else(|| digits(v.as_str()?)))
        .or_else(|| first("dl.rating .rating-count").and_then(|t| digits(&t)));
    let skip = [
        "電話番号",
        "住所",
        "アクセス",
        "営業時間",
        "定休日",
        "カット価格",
    ];
    let details: Vec<(String, String)> = table
        .iter()
        .filter(|(k, _)| !skip.iter().any(|s| k.starts_with(s)))
        .cloned()
        .collect();
    let extra = format!(
        "{} {}",
        first("p.detailTitleKana").unwrap_or_default(),
        field("こだわり条件").unwrap_or_default()
    );
    let badges: Vec<String> = doc
        .select(&sel("ul.slnInfoLabels .slnInfoLabel"))
        .map(text)
        .filter(|b| !b.is_empty())
        .collect();
    let catch = first("p.shopCatchCopy");
    let blurb = first("div.slnTopImgDescription > p");
    let description = match (catch, blurb) {
        (Some(c), Some(b)) => Some(format!("{c}\n\n{b}")),
        (c, b) => c.or(b),
    };
    let id = salon
        .and_then(|s| str_of(s, "url"))
        .and_then(parse_id)
        .or_else(|| {
            doc.select(&sel("a[href*='storeId=']"))
                .find_map(|a| parse_id(a.value().attr("href")?))
        })
        .or_else(|| {
            ld_type(&lds, "BreadcrumbList")
                .and_then(|b| b.get("itemListElement")?.as_array()?.last().cloned())
                .and_then(|l| parse_id(str_of(l.get("item")?, "@id")?))
        })
        .ok_or_else(|| anyhow!("HotPepper Beauty salon page has no salon id"))?;
    let booking_url = doc
        .select(&sel("a[href*='/CSP/bt/reserve/']"))
        .next()
        .and_then(|a| a.value().attr("href"))
        .map(|h| absolute(&strip_query_keep_store(h)));
    let image = doc
        .select(&sel(".slnTopImgViewer img"))
        .next()
        .and_then(|i| i.value().attr("src"))
        .map(str::to_string)
        .or_else(|| salon.and_then(|s| str_of(s, "image")).map(str::to_string));
    let menus = match coupons {
        Some(c) => parse_menus(c)?,
        None => Vec::new(),
    };

    Ok(Venue {
        url: salon_url(&id),
        genre: genre(&name, &extra),
        area: breadcrumb_area(&lds),
        address: field("住所").or_else(|| salon.and_then(|s| str_of(s, "address")).map(tidy)),
        access: field("アクセス"),
        rating,
        rating_count,
        budget: field("カット価格").map(|p| format!("カット {p}")),
        hours: field("営業時間"),
        closed: field("定休日"),
        phone,
        booking_url,
        badges,
        details,
        menus,
        description,
        image,
        id,
        name,
    })
}

pub fn parse_menus(body: &str) -> Result<Vec<Menu>> {
    if let Some(why) = is_blocked(body) {
        return Err(FetchError::Blocked(format!("HotPepper Beauty: {why}")).into());
    }
    let doc = Html::parse_document(body);
    let name = sel(".couponMenuName");
    let one = |e: ElementRef, s: &Selector| e.select(s).next().map(text).filter(|t| !t.is_empty());
    let mut menus = Vec::new();

    let coupon_price = sel(".couponMenuPrice");
    let label = sel("td[class^='couponLabel']");
    let limited = sel("ul.limitedCouponIcon li");
    let coupon_desc = sel(".couponDescription");
    for t in doc.select(&sel("table.couponTable")) {
        let Some(n) = one(t, &name) else {
            continue;
        };
        let price_text = one(t, &coupon_price);
        let mut tags: Vec<String> = one(t, &label)
            .map(|l| l.replace(' ', ""))
            .into_iter()
            .collect();
        tags.extend(t.select(&limited).map(text).filter(|l| !l.is_empty()));
        let mut desc: Vec<String> = Vec::new();
        if !tags.is_empty() {
            desc.push(format!("[{}]", tags.join("・")));
        }
        desc.extend(one(t, &coupon_desc).map(|d| tidy(&d)));
        menus.push(Menu {
            name: n,
            price: price_text.as_deref().and_then(yen),
            price_text,
            description: Some(desc.join(" ")).filter(|d| !d.is_empty()),
        });
    }

    let menu_price = sel("p.wwbw span");
    let menu_desc = sel("p.wbba");
    for t in doc.select(&sel("table.menuTbl")) {
        let Some(n) = one(t, &name) else {
            continue;
        };
        let price_text = one(t, &menu_price);
        menus.push(Menu {
            name: n,
            price: price_text.as_deref().and_then(yen),
            price_text,
            description: one(t, &menu_desc).map(|d| tidy(&d)),
        });
    }
    Ok(menus)
}

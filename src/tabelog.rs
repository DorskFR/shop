use anyhow::{Context, Result, bail};
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher, Response},
    venue::{Connector, Menu, Query, SearchPage, Sort, Venue},
};

const SITE: &str = "https://tabelog.com";
const PREFECTURES: [&str; 47] = [
    "hokkaido",
    "aomori",
    "iwate",
    "miyagi",
    "akita",
    "yamagata",
    "fukushima",
    "ibaraki",
    "tochigi",
    "gunma",
    "saitama",
    "chiba",
    "tokyo",
    "kanagawa",
    "niigata",
    "toyama",
    "ishikawa",
    "fukui",
    "yamanashi",
    "nagano",
    "gifu",
    "shizuoka",
    "aichi",
    "mie",
    "shiga",
    "kyoto",
    "osaka",
    "hyogo",
    "nara",
    "wakayama",
    "tottori",
    "shimane",
    "okayama",
    "hiroshima",
    "yamaguchi",
    "tokushima",
    "kagawa",
    "ehime",
    "kochi",
    "fukuoka",
    "saga",
    "nagasaki",
    "kumamoto",
    "oita",
    "miyazaki",
    "kagoshima",
    "okinawa",
];
const HANDLED: [&str; 11] = [
    "店名",
    "受賞・選出歴",
    "ジャンル",
    "予約・お問い合わせ",
    "お問い合わせ",
    "住所",
    "交通手段",
    "営業時間",
    "予算",
    "初投稿者",
    "最近の編集者",
];
const JUNK: [&str; 7] = [
    "大きな地図を見る",
    "周辺のお店を探す",
    "利用金額分布を見る",
    "編集履歴を詳しく見る",
    "こんな時によく使われます。",
    "詳しくはこちら",
    "過去受賞を見る",
];

pub struct Tabelog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    pub name: String,
    pub datatype: String,
    pub id: u64,
}

pub fn suggest_url(area: &str) -> Result<String> {
    let mut url = url::Url::parse(&format!("{SITE}/internal_api/suggest_form_words"))?;
    url.query_pairs_mut().append_pair("sa", area);
    Ok(url.into())
}

pub fn parse_suggest(body: &str) -> Result<Vec<Area>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("Tabelog area suggest returned a non-JSON body: {e}"))?;
    let Some(list) = v.as_array() else {
        bail!("Tabelog area suggest returned an unexpected JSON shape");
    };
    Ok(list
        .iter()
        .filter_map(|a| {
            Some(Area {
                name: str_of(a, "name")?.to_string(),
                datatype: str_of(a, "datatype")?.to_string(),
                id: a.get("id_in_datatype")?.as_u64()?,
            })
        })
        .collect())
}

pub fn sort_code(sort: Sort) -> Option<&'static str> {
    match sort {
        Sort::Relevance => None,
        Sort::Rating => Some("rt"),
        Sort::Reviews => Some("rvcn"),
        Sort::Newest => Some("nod"),
    }
}

pub fn search_url(q: &Query, area: Option<&Area>) -> Result<String> {
    let mut url = url::Url::parse(&format!("{SITE}/rst/rstsearch"))?;
    {
        let mut p = url.query_pairs_mut();
        p.append_pair("sk", q.text);
        if let Some(a) = area {
            p.append_pair("sa", &a.name);
            p.append_pair("area_datatype", &a.datatype);
            p.append_pair("area_id", &a.id.to_string());
        }
        if let Some(code) = sort_code(q.sort) {
            p.append_pair("SrtT", code);
        }
    }
    Ok(url.into())
}

pub fn page_url(first_page: &str, page: u32) -> Option<String> {
    let doc = Html::parse_document(first_page);
    let href = doc
        .select(&sel("a.c-pagination__num"))
        .find_map(|a| a.value().attr("href"))?;
    let mut url = url::Url::parse(SITE).ok()?.join(href).ok()?;
    let segs: Vec<String> = url
        .path_segments()?
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let (last, head) = segs.split_last()?;
    if !last.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    url.set_path(&format!("/{}/{page}/", head.join("/")));
    Some(url.into())
}

pub fn parse_id(input: &str) -> Option<String> {
    let input = input.trim();
    if input.len() >= 7 && input.bytes().all(|b| b.is_ascii_digit()) {
        let code: usize = input[..2].parse().ok()?;
        let pref = PREFECTURES.get(code.checked_sub(1)?)?;
        return Some(format!("{pref}/A{:02}01/A{:02}0101/{input}", code, code));
    }
    let path = match url::Url::parse(input) {
        Ok(url) => {
            if !url.host_str()?.ends_with("tabelog.com") {
                return None;
            }
            url.path().to_string()
        }
        Err(_) => input.to_string(),
    };
    let mut segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs
        .first()
        .is_some_and(|s| matches!(*s, "en" | "cn" | "kr" | "tw"))
    {
        segs.remove(0);
    }
    let [pref, a, aa, rcd, ..] = segs[..] else {
        return None;
    };
    let area_code = |s: &str, n: usize| {
        s.len() == n + 1 && s.starts_with('A') && s[1..].bytes().all(|b| b.is_ascii_digit())
    };
    let ok = PREFECTURES.contains(&pref)
        && area_code(a, 4)
        && area_code(aa, 6)
        && !rcd.is_empty()
        && rcd.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| format!("{pref}/{a}/{aa}/{rcd}"))
}

pub fn item_url(id: &str) -> String {
    format!("{SITE}/{id}/")
}

pub fn is_challenge(body: &str) -> bool {
    body.contains("<title>Just a moment...</title>")
        || body.contains("<title>Attention Required! | Cloudflare</title>")
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn text(e: ElementRef) -> String {
    squash(&e.text().collect::<String>())
}

fn digits(s: &str) -> Option<u64> {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.parse().ok()
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn skipped(e: &scraper::node::Element) -> bool {
    matches!(e.name(), "script" | "style" | "button" | "img")
        || e.classes().any(|c| {
            matches!(
                c,
                "rstinfo-table__open-closed-notice"
                    | "rstinfo-table__notice-attention"
                    | "u-text-notice"
            ) || c.starts_with("c-tooltip")
                || c.starts_with("c-modal")
                || c.starts_with("pr-comment__more")
                || c.contains("map-wrap")
        })
}

fn walk(e: ElementRef, out: &mut Vec<String>) {
    for child in e.children() {
        match child.value() {
            Node::Text(t) => {
                if let Some(last) = out.last_mut() {
                    last.push_str(t);
                }
            }
            Node::Element(el) if skipped(el) => {}
            Node::Element(el) => {
                let block = matches!(
                    el.name(),
                    "p" | "div" | "li" | "ul" | "ol" | "br" | "tr" | "dt" | "dd" | "h3" | "h4"
                );
                if block {
                    out.push(String::new());
                }
                if let Some(c) = ElementRef::wrap(child) {
                    walk(c, out);
                }
                if block {
                    out.push(String::new());
                }
            }
            _ => {}
        }
    }
}

fn lines(e: ElementRef) -> Vec<String> {
    let mut out = vec![String::new()];
    walk(e, &mut out);
    out.iter()
        .map(|l| squash(&l.replace('\u{3000}', " ")))
        .filter(|l| !l.is_empty() && !JUNK.contains(&l.as_str()))
        .collect()
}

fn budget_of(e: ElementRef) -> Option<String> {
    let parts: Vec<String> = e
        .select(&sel("i.c-rating-v3__time"))
        .filter_map(|i| {
            let label = if i.value().has_class(
                "c-rating-v3__time--lunch",
                scraper::CaseSensitivity::CaseSensitive,
            ) {
                "昼"
            } else {
                "夜"
            };
            let parent = ElementRef::wrap(i.parent()?)?;
            let val = text(parent);
            (!val.is_empty() && val != "-").then(|| format!("{label} {val}"))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" / "))
}

fn split_genres(s: &str) -> Vec<String> {
    s.split(['、', ','])
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_string)
        .collect()
}

fn award_badges<'a>(nodes: impl Iterator<Item = ElementRef<'a>>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for i in nodes {
        if i.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|e| e.classes().any(|c| c.starts_with("c-modal")))
        }) {
            continue;
        }
        let t = text(i);
        if t.is_empty() {
            continue;
        }
        let key: String = t.chars().filter(|c| !c.is_ascii_digit()).collect();
        if !seen.contains(&key) {
            seen.push(key);
            out.push(t);
        }
    }
    out
}

fn ensure_page(body: &str) -> Result<()> {
    if is_challenge(body) {
        return Err(FetchError::Blocked("Tabelog: Cloudflare challenge".into()).into());
    }
    Ok(())
}

pub fn parse_search(body: &str) -> Result<SearchPage> {
    ensure_page(body)?;
    let doc = Html::parse_document(body);
    let count = doc.select(&sel(".c-page-count")).next();
    let cassettes: Vec<ElementRef> = doc.select(&sel("div.list-rst[data-rst-id]")).collect();
    if count.is_none() && cassettes.is_empty() {
        bail!("Tabelog search page layout not recognised");
    }
    let total = count.and_then(|c| {
        let t = text(c);
        let (_, tail) = t.rsplit_once('全')?;
        digits(tail)
    });
    let page = doc
        .select(&sel(".c-pagination__num.is-current"))
        .next()
        .and_then(|e| digits(&text(e)))
        .map_or(1, |p| p as u32);

    let name_sel = sel("a.list-rst__rst-name-target");
    let area_genre = sel(".list-rst__area-genre");
    let mark = sel("mark");
    let rating = sel(".list-rst__rating-val");
    let reviews = sel(".list-rst__rvw-count-num");
    let info = sel(".list-rst__info");
    let holiday = sel(".list-rst__holiday-text");
    let badge = sel(
        ".list-rst__award-badge .c-badge-hyakumeiten i, .list-rst__award-badge .c-badge-award i",
    );
    let cover = sel(".list-rst__photo-item--cover[data-original]");
    let calendar = sel(".list-rst__calendar-frame");

    let mut items = Vec::new();
    for c in cassettes {
        let Some(a) = c.select(&name_sel).next() else {
            continue;
        };
        let url = a
            .value()
            .attr("href")
            .or_else(|| c.value().attr("data-detail-url"))
            .unwrap_or_default()
            .to_string();
        let (area, genre) = match c.select(&area_genre).next() {
            Some(ag) => {
                let full = text(ag);
                let (area, genre) = full.split_once(" / ").unwrap_or((full.as_str(), ""));
                let marked = ag.select(&mark).next().map(text);
                let genre = split_genres(genre)
                    .into_iter()
                    .map(
                        |g| match marked.as_deref().and_then(|m| g.strip_prefix(m)) {
                            Some(rest) if rest.starts_with('(') && rest.ends_with(')') => {
                                rest[1..rest.len() - 1].to_string()
                            }
                            _ => g,
                        },
                    )
                    .collect();
                (
                    Some(area.trim().to_string()).filter(|a| !a.is_empty()),
                    genre,
                )
            }
            None => (None, Vec::new()),
        };
        let mut badges = award_badges(c.select(&badge));
        let bookable = c.select(&calendar).next().is_some();
        if bookable {
            badges.push("ネット予約".into());
        }
        items.push(Venue {
            id: c
                .value()
                .attr("data-rst-id")
                .unwrap_or_default()
                .to_string(),
            name: text(a),
            booking_url: bookable.then(|| url.clone()),
            url,
            genre,
            area,
            rating: c
                .select(&rating)
                .next()
                .and_then(|r| text(r).parse::<f64>().ok()),
            rating_count: c.select(&reviews).next().and_then(|r| digits(&text(r))),
            budget: c.select(&info).next().and_then(budget_of),
            closed: c
                .select(&holiday)
                .next()
                .map(text)
                .filter(|t| !t.is_empty()),
            badges,
            image: c
                .select(&cover)
                .next()
                .and_then(|i| i.value().attr("data-original"))
                .map(str::to_string),
            ..Venue::default()
        });
    }
    Ok(SearchPage { total, page, items })
}

fn restaurant_ld(doc: &Html) -> Option<Value> {
    doc.select(&sel(r#"script[type="application/ld+json"]"#))
        .filter_map(|s| serde_json::from_str::<Value>(&s.text().collect::<String>()).ok())
        .find(|v| str_of(v, "@type") == Some("Restaurant"))
}

fn phone_like(s: &str) -> Option<String> {
    let s = s.trim();
    let ok = s.chars().filter(char::is_ascii_digit).count() >= 6
        && s.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '(' | ')' | '+' | ' '));
    ok.then(|| s.to_string())
}

fn hours_and_closed(td: ElementRef) -> (Option<String>, Option<String>) {
    let item_sel = sel("li.rstinfo-table__business-item");
    let title_sel = sel(".rstinfo-table__business-title");
    let dtl_sel = sel("li.rstinfo-table__business-dtl-text");
    let other_sel = sel(".rstinfo-table__business-other");
    let mut hours: Vec<String> = Vec::new();
    let mut closed: Vec<String> = Vec::new();
    for item in td.select(&item_sel) {
        if item.ancestors().any(|a| {
            a.value().as_element().is_some_and(|e| {
                e.has_class(
                    "rstinfo-table__business-other",
                    scraper::CaseSensitivity::CaseSensitive,
                )
            })
        }) {
            continue;
        }
        let title = item.select(&title_sel).next().map(text).unwrap_or_default();
        let slots: Vec<String> = item
            .select(&dtl_sel)
            .map(|d| {
                let l = lines(d);
                match l.split_first() {
                    Some((time, rest)) if !rest.is_empty() => {
                        format!("{time} ({})", rest.join(", "))
                    }
                    _ => l.join(" "),
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        if slots.is_empty() && title.is_empty() {
            continue;
        }
        if slots.iter().all(|s| s == "定休日") && !slots.is_empty() && !title.is_empty() {
            closed.push(title);
        } else {
            hours.push(format!("{title} {}", slots.join(", ")).trim().to_string());
        }
    }
    for other in td.select(&other_sel) {
        for li in other.select(&item_sel) {
            let l = lines(li);
            let (head, body) = match l.split_first() {
                Some((h, b)) if h.starts_with('■') => (h.trim_start_matches('■').trim(), b),
                _ => ("", &l[..]),
            };
            match head {
                "定休日" => closed.extend(body.iter().cloned()),
                "" | "営業時間" => hours.extend(body.iter().cloned()),
                h => hours.push(format!("{h}: {}", body.join(" "))),
            }
        }
    }
    let join = |v: Vec<String>| Some(v.join("\n")).filter(|s| !s.is_empty());
    (join(hours), join(closed))
}

pub fn parse_item(body: &str) -> Result<Venue> {
    ensure_page(body)?;
    let doc = Html::parse_document(body);
    let title = doc
        .select(&sel("title"))
        .next()
        .map(text)
        .unwrap_or_default();
    if title.contains("お探しのページが見つかりません") {
        return Err(FetchError::NotFound("Tabelog restaurant".into()).into());
    }
    let name_el = doc.select(&sel("h2.display-name")).next();
    let info_el = doc.select(&sel("#rst-data-head")).next();
    if name_el.is_none() && info_el.is_none() {
        bail!("Tabelog restaurant page layout not recognised");
    }
    let ld = restaurant_ld(&doc).unwrap_or(Value::Null);

    let mut rows: Vec<(String, ElementRef)> = Vec::new();
    let th_sel = sel("th");
    let td_sel = sel("td");
    for tr in doc.select(&sel("table.rstinfo-table__table tr")) {
        if let (Some(th), Some(td)) = (tr.select(&th_sel).next(), tr.select(&td_sel).next()) {
            rows.push((text(th).replace(' ', ""), td));
        }
    }
    let row = |k: &str| rows.iter().find(|(n, _)| n == k).map(|(_, td)| *td);

    let url = doc
        .select(&sel(r#"link[rel="canonical"]"#))
        .next()
        .and_then(|l| l.value().attr("href"))
        .or_else(|| str_of(&ld, "@id"))
        .map(str::to_string)
        .context("Tabelog restaurant page has no canonical URL")?;
    let id = parse_id(&url)
        .and_then(|p| p.rsplit('/').next().map(str::to_string))
        .unwrap_or_default();

    let name = name_el
        .map(text)
        .filter(|n| !n.is_empty())
        .or_else(|| str_of(&ld, "name").map(str::to_string))
        .or_else(|| row("店名").map(text))
        .unwrap_or_default();

    let agg = ld.get("aggregateRating").unwrap_or(&Value::Null);
    let rating = doc
        .select(&sel(".rdheader-rating__score-val-dtl"))
        .next()
        .and_then(|e| text(e).parse::<f64>().ok())
        .or_else(|| str_of(agg, "ratingValue").and_then(|v| v.parse().ok()));
    let rating_count = doc
        .select(&sel(".rdheader-rating__review-target em.num"))
        .next()
        .and_then(|e| digits(&text(e)))
        .or_else(|| str_of(agg, "ratingCount").and_then(digits));

    let genre = row("ジャンル")
        .map(|td| split_genres(&text(td)))
        .filter(|g| !g.is_empty())
        .or_else(|| str_of(&ld, "servesCuisine").map(split_genres))
        .unwrap_or_default();

    let area = doc
        .select(&sel(
            ".rdheader-subinfo__item--station .linktree__parent-target-text",
        ))
        .next()
        .map(text)
        .filter(|a| !a.is_empty());

    let address = doc
        .select(&sel("p.rstinfo-table__address"))
        .next()
        .map(text)
        .filter(|a| !a.is_empty());

    let access = row("交通手段")
        .map(|td| lines(td).join("\n"))
        .filter(|a| !a.is_empty());

    let (hours, closed) = row("営業時間").map_or((None, None), hours_and_closed);

    let phone = str_of(&ld, "telephone").and_then(phone_like).or_else(|| {
        rows.iter()
            .filter(|(k, _)| k.contains("お問い合わせ"))
            .find_map(|(_, td)| phone_like(&text(*td)))
    });

    let booking_url = doc
        .select(&sel(r#"#js-booking-params[data-booking-enabled="1"]"#))
        .next()
        .map(|_| url.clone());

    let budget = doc
        .select(&sel(".rdheader-budget"))
        .next()
        .and_then(budget_of)
        .or_else(|| row("予算").and_then(budget_of));

    let badges = row("受賞・選出歴").map_or_else(Vec::new, |td| {
        award_badges(td.select(&sel(".c-badge-hyakumeiten i, .c-badge-award i")))
    });

    let mut details: Vec<(String, String)> = Vec::new();
    for (k, td) in &rows {
        if HANDLED.contains(&k.as_str()) {
            continue;
        }
        let val = if k.starts_with("予算") {
            budget_of(*td)
        } else {
            Some(lines(*td).iter().fold(String::new(), |acc, l| {
                if acc.is_empty() {
                    l.clone()
                } else if l.starts_with(['（', '(']) {
                    format!("{acc} {l}")
                } else {
                    format!("{acc} / {l}")
                }
            }))
        };
        if let Some(val) = val.filter(|v| !v.is_empty()) {
            details.push((k.clone(), val));
        }
    }

    let description = doc.select(&sel(".pr-comment-wrap")).next().and_then(|pr| {
        let mut out: Vec<String> = pr
            .select(&sel(".pr-comment-title"))
            .next()
            .map(text)
            .into_iter()
            .collect();
        if let Some(b) = pr.select(&sel(".pr-comment__body")).next() {
            out.extend(lines(b));
        }
        Some(out.join("\n")).filter(|d| !d.is_empty())
    });

    let menus = doc
        .select(&sel(".rstdtl-top-course .rstdtl-course-list"))
        .filter_map(|c| {
            let name = c
                .select(&sel(".rstdtl-course-list__course-title-text"))
                .next()
                .map(text)
                .filter(|n| !n.is_empty())?;
            let price_el = c.select(&sel(".rstdtl-course-list__price-num")).next();
            let label = c
                .select(&sel(".rstdtl-course-list__label"))
                .next()
                .map(text);
            let desc = c.select(&sel(".rstdtl-course-list__desc")).next().map(text);
            let description: Vec<String> = label
                .into_iter()
                .chain(desc)
                .filter(|s| !s.is_empty())
                .collect();
            Some(Menu {
                name,
                price: price_el
                    .and_then(|p| p.select(&sel("em")).next())
                    .and_then(|e| digits(&text(e))),
                price_text: price_el.map(text).filter(|t| !t.is_empty()),
                description: Some(description.join(" / ")).filter(|d| !d.is_empty()),
            })
        })
        .collect();

    let image = str_of(&ld, "image").map(str::to_string).or_else(|| {
        doc.select(&sel(r#"meta[property="og:image"]"#))
            .next()
            .and_then(|m| m.value().attr("content"))
            .map(str::to_string)
    });

    Ok(Venue {
        id,
        name,
        url,
        genre,
        area,
        address,
        access,
        rating,
        rating_count,
        budget,
        hours,
        closed,
        phone,
        booking_url,
        badges,
        details,
        menus,
        description,
        image,
    })
}

fn fetch(fetcher: &Fetcher, url: &str, accept: &str) -> Result<Response> {
    let r = fetcher.get(
        url,
        &[("Accept", accept), ("Accept-Language", "ja-JP,ja;q=0.9")],
    )?;
    match r.status {
        403 | 429 | 503 => Err(FetchError::Blocked(format!("Tabelog: HTTP {}", r.status)).into()),
        _ if is_challenge(&r.body) => {
            Err(FetchError::Blocked("Tabelog: Cloudflare challenge".into()).into())
        }
        404 => Err(FetchError::NotFound(url.to_string()).into()),
        s if s >= 400 => bail!("Tabelog returned HTTP {s} for {url}"),
        _ => Ok(r),
    }
}

fn resolve_area(fetcher: &Fetcher, area: &str) -> Result<Area> {
    let r = fetch(fetcher, &suggest_url(area)?, "application/json")?;
    parse_suggest(&r.body)?
        .into_iter()
        .next()
        .ok_or_else(|| FetchError::NotFound(format!("Tabelog area '{area}'")).into())
}

impl Connector for Tabelog {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage> {
        let area = query.area.map(|a| resolve_area(fetcher, a)).transpose()?;
        let url = search_url(query, area.as_ref())?;
        let first = fetch(fetcher, &url, "text/html")?.body;
        let page = parse_search(&first)?;
        if query.page <= 1 {
            return Ok(page);
        }
        let Some(next) = page_url(&first, query.page) else {
            return Ok(SearchPage {
                total: page.total,
                page: query.page,
                items: Vec::new(),
            });
        };
        let mut res = parse_search(&fetch(fetcher, &next, "text/html")?.body)?;
        res.page = query.page;
        Ok(res)
    }

    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Venue> {
        let Some(id) = parse_id(id_or_url) else {
            return Err(FetchError::NotFound(format!(
                "'{id_or_url}' is not a Tabelog restaurant id or tabelog.com/<pref>/A..../A....../<id>/ URL"
            ))
            .into());
        };
        let r = fetch(fetcher, &item_url(&id), "text/html")?;
        parse_item(&r.body)
    }
}

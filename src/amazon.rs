use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Market {
    Jp,
    Com,
}

impl Market {
    pub fn host(self) -> &'static str {
        match self {
            Market::Jp => "www.amazon.co.jp",
            Market::Com => "www.amazon.com",
        }
    }

    pub fn accept_language(self) -> &'static str {
        match self {
            Market::Jp => "ja-JP,ja;q=0.9,en;q=0.5",
            Market::Com => "en-US,en;q=0.9",
        }
    }

    pub fn from_host(host: &str) -> Option<Market> {
        let host = host.trim_start_matches("www.");
        match host {
            "amazon.co.jp" => Some(Market::Jp),
            "amazon.com" => Some(Market::Com),
            _ => None,
        }
    }

    pub fn product_url(self, asin: &str) -> String {
        format!("https://{}/dp/{asin}", self.host())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sort {
    Relevance,
    PriceAsc,
    PriceDesc,
    Reviews,
    Newest,
}

impl Sort {
    fn param(self) -> Option<&'static str> {
        match self {
            Sort::Relevance => None,
            Sort::PriceAsc => Some("price-asc-rank"),
            Sort::PriceDesc => Some("price-desc-rank"),
            Sort::Reviews => Some("review-rank"),
            Sort::Newest => Some("date-desc-rank"),
        }
    }
}

pub fn search_url(market: Market, query: &str, page: u32, sort: Sort) -> String {
    let mut url = url::Url::parse(&format!("https://{}/s", market.host())).expect("static url");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("k", query);
        if page > 1 {
            q.append_pair("page", &page.to_string());
        }
        if let Some(s) = sort.param() {
            q.append_pair("s", s);
        }
    }
    url.into()
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageKind {
    Ok,
    Blocked(&'static str),
    NotFound,
}

pub fn classify(html: &str) -> PageKind {
    if html.contains("validateCaptcha") {
        return PageKind::Blocked("captcha");
    }
    if html.contains("Sorry! Something went wrong!") {
        return PageKind::Blocked("error page (503)");
    }
    if html.contains("<title>Page Not Found</title>")
        || html.contains("ページが見つかりません")
        || (html.contains("Page Not Found") && html.len() < 20_000)
    {
        return PageKind::NotFound;
    }
    if html.len() < 20_000 && !html.contains("productTitle") && !html.contains("s-search-result") {
        return PageKind::Blocked("challenge page");
    }
    PageKind::Ok
}

pub fn parse_asin(input: &str) -> Option<(String, Option<Market>)> {
    let input = input.trim();
    if is_asin(input) {
        return Some((input.to_ascii_uppercase(), None));
    }
    let url = url::Url::parse(input).ok()?;
    let market = url.host_str().and_then(Market::from_host);
    let segments: Vec<&str> = url.path_segments()?.collect();
    segments
        .windows(2)
        .find(|w| matches!(w[0], "dp" | "product" | "d") && is_asin(w[1]))
        .map(|w| (w[1].to_ascii_uppercase(), market))
}

fn is_asin(s: &str) -> bool {
    s.len() == 10 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub asin: String,
    pub title: String,
    pub url: String,
    pub price: Option<String>,
    pub price_value: Option<f64>,
    pub list_price: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,
    pub bought_past_month: Option<String>,
    pub sponsored: bool,
    pub badges: Vec<String>,
    pub delivery: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Item {
    pub asin: String,
    pub url: String,
    pub title: String,
    pub brand: Option<String>,
    pub price: Option<String>,
    pub price_value: Option<f64>,
    pub list_price: Option<String>,
    pub availability: Option<String>,
    pub seller: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,
    pub histogram: Vec<HistogramRow>,
    pub categories: Vec<String>,
    pub features: Vec<String>,
    pub details: Vec<(String, String)>,
    pub image: Option<String>,
    pub reviews: Vec<Review>,
}

#[derive(Debug, Serialize)]
pub struct HistogramRow {
    pub stars: u8,
    pub percent: u8,
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub id: Option<String>,
    pub author: Option<String>,
    pub rating: Option<f64>,
    pub title: Option<String>,
    pub date: Option<String>,
    pub verified: bool,
    pub variant: Option<String>,
    pub helpful: Option<String>,
    pub body: String,
}

macro_rules! sel {
    ($name:ident, $css:expr) => {
        static $name: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse($css).expect(concat!("selector ", $css)));
    };
}

sel!(RESULT, r#"div[data-component-type="s-search-result"]"#);
sel!(H2, "h2");
sel!(MAIN_PRICE, ".a-price:not(.a-text-price) > .a-offscreen");
sel!(
    LIST_PRICE,
    r#".a-price.a-text-price[data-a-strike="true"] > .a-offscreen"#
);
sel!(SPONSORED, ".puis-sponsored-label-text");
sel!(BADGE, ".rio-badge-label, .a-badge-label-inner");
sel!(REVIEWS_BLOCK, r#"[data-cy="reviews-block"]"#);
sel!(STAR_ALT, ".a-icon-alt");
sel!(ARIA_LINK, "a[aria-label]");
sel!(SECONDARY, ".a-size-base.a-color-secondary");
sel!(DELIVERY, r#"[data-cy="delivery-recipe"]"#);
sel!(S_IMAGE, "img.s-image");

pub fn parse_search(html: &str, market: Market) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    doc.select(&RESULT)
        .filter_map(|card| {
            let asin = card.attr("data-asin")?.trim().to_string();
            if asin.is_empty() {
                return None;
            }
            let title = card.select(&H2).next().map(text)?;
            let price = first_text(card, &MAIN_PRICE);
            let reviews = card.select(&REVIEWS_BLOCK).next();
            let rating = reviews
                .and_then(|r| first_text(r, &STAR_ALT))
                .and_then(|t| parse_rating(&t));
            let rating_count = reviews.and_then(|r| {
                r.select(&ARIA_LINK)
                    .filter_map(|a| a.attr("aria-label"))
                    .filter(|l| !l.contains("5つ星") && !l.contains("out of 5"))
                    .find_map(parse_count)
            });
            let bought_past_month = reviews.and_then(|r| {
                r.select(&SECONDARY)
                    .map(text)
                    .find(|t| t.contains("bought") || t.contains("購入"))
            });
            Some(SearchResult {
                url: market.product_url(&asin),
                asin,
                title,
                price_value: price.as_deref().and_then(parse_money),
                price,
                list_price: first_text(card, &LIST_PRICE)
                    .map(|t| strip_label(&t))
                    .filter(|t| !t.is_empty()),
                rating,
                rating_count,
                bought_past_month,
                sponsored: card.select(&SPONSORED).next().is_some(),
                badges: dedup(card.select(&BADGE).map(text).filter(|t| !t.is_empty())),
                delivery: first_text(card, &DELIVERY),
                image: card
                    .select(&S_IMAGE)
                    .next()
                    .and_then(|i| i.attr("src"))
                    .map(str::to_string),
            })
        })
        .collect()
}

sel!(TITLE, "#productTitle");
sel!(BYLINE, "#bylineInfo");
sel!(
    CORE_PRICE,
    "#corePrice_feature_div .a-price:not(.a-text-price) .a-offscreen, \
     #corePriceDisplay_desktop_feature_div .priceToPay .a-offscreen, \
     #corePrice_desktop .a-price .a-offscreen"
);
sel!(
    CORE_LIST_PRICE,
    "#corePriceDisplay_desktop_feature_div .basisPrice .a-offscreen, \
     #corePrice_desktop .a-text-price .a-offscreen"
);
sel!(AVAILABILITY, "#availability > span, #availability");
sel!(
    SELLER,
    "#merchantInfoFeature_feature_div .offer-display-feature-text-message, #sellerProfileTriggerId"
);
sel!(ACR_POPOVER, "#acrPopover");
sel!(ACR_COUNT, "#acrCustomerReviewText");
sel!(
    HISTOGRAM_LINK,
    "#histogramTable a[aria-label], #histogramTable li[aria-label]"
);
sel!(BREADCRUMB, "#wayfinding-breadcrumbs_feature_div li a");
sel!(FEATURE, "#feature-bullets li span.a-list-item");
sel!(
    DETAIL_ROW,
    "#productOverview_feature_div tr, #prodDetails tr, #productDetails_techSpec_section_1 tr"
);
sel!(TH_TD_FIRST, "th, td:first-child");
sel!(TD_LAST, "td:last-child");
sel!(
    DETAIL_BULLET,
    "#detailBullets_feature_div li span.a-list-item"
);
sel!(BOLD, ".a-text-bold");
sel!(LANDING_IMAGE, "#landingImage");
sel!(REVIEW, r#"[data-hook="review"]"#);
sel!(PROFILE_NAME, ".a-profile-name");
sel!(
    REVIEW_STARS,
    r#"[data-hook="review-star-rating"] .a-icon-alt, [data-hook="cmps-review-star-rating"] .a-icon-alt"#
);
sel!(
    REVIEW_TITLE,
    r#"[data-hook="reviewTitle"], [data-hook="review-title"]"#
);
sel!(REVIEW_DATE, r#"[data-hook="review-date"]"#);
sel!(AVP, r#"[data-hook="avp-badge"]"#);
sel!(
    VARIANT,
    r#"[data-hook="format-strip-linkless"], [data-hook="format-strip"]"#
);
sel!(HELPFUL, r#"[data-hook="helpful-vote-statement"]"#);
sel!(
    REVIEW_BODY,
    r#"[data-hook="reviewRichContentContainer"], [data-hook="review-body"]"#
);
sel!(REPORT, "[data-id]");

const SKIP_DETAILS: &[&str] = &["おすすめ度", "Customer Reviews"];

pub fn parse_item(html: &str, market: Market, asin: &str) -> Item {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let price = first_text(root, &CORE_PRICE);
    let mut details: Vec<(String, String)> = Vec::new();
    for row in root.select(&DETAIL_ROW) {
        let (Some(k), Some(v)) = (
            row.select(&TH_TD_FIRST).next().map(text),
            row.select(&TD_LAST).next().map(text),
        ) else {
            continue;
        };
        if !k.is_empty()
            && !v.is_empty()
            && k != v
            && !SKIP_DETAILS.contains(&k.as_str())
            && !details.iter().any(|(dk, _)| *dk == k)
        {
            details.push((k, v));
        }
    }
    for li in root.select(&DETAIL_BULLET) {
        let Some(k) = li.select(&BOLD).next().map(text) else {
            continue;
        };
        let full = text(li);
        let v = full.strip_prefix(k.as_str()).unwrap_or(&full).trim();
        let k = k
            .trim_end_matches([':', '：', '‏', '‎', ' '])
            .trim()
            .to_string();
        if !k.is_empty() && !v.is_empty() && !details.iter().any(|(dk, _)| *dk == k) {
            details.push((k, v.to_string()));
        }
    }
    Item {
        asin: asin.to_string(),
        url: market.product_url(asin),
        title: first_text(root, &TITLE).unwrap_or_default(),
        brand: first_text(root, &BYLINE),
        price_value: price.as_deref().and_then(parse_money),
        price,
        list_price: first_text(root, &CORE_LIST_PRICE).map(|t| strip_label(&t)),
        availability: first_text(root, &AVAILABILITY).map(|t| clean_availability(&t)),
        seller: first_text(root, &SELLER),
        rating: root
            .select(&ACR_POPOVER)
            .next()
            .and_then(|e| e.attr("title"))
            .and_then(parse_rating),
        rating_count: root
            .select(&ACR_COUNT)
            .next()
            .and_then(|e| e.attr("aria-label").map(str::to_string).or(Some(text(e))))
            .and_then(|t| parse_count(&t)),
        histogram: parse_histogram(root),
        categories: dedup(root.select(&BREADCRUMB).map(text)),
        features: dedup(root.select(&FEATURE).map(text).filter(|t| !t.is_empty())),
        details,
        image: root.select(&LANDING_IMAGE).next().and_then(|i| {
            i.attr("data-old-hires")
                .filter(|s| !s.is_empty())
                .or_else(|| i.attr("src"))
                .map(str::to_string)
        }),
        reviews: root.select(&REVIEW).map(parse_review).collect(),
    }
}

fn parse_histogram(root: ElementRef) -> Vec<HistogramRow> {
    let mut rows: Vec<HistogramRow> = Vec::new();
    for label in root
        .select(&HISTOGRAM_LINK)
        .filter_map(|e| e.attr("aria-label"))
    {
        let Some(percent) = number_before(label, '%') else {
            continue;
        };
        let stars = 5u8.saturating_sub(rows.len() as u8);
        if stars == 0 {
            break;
        }
        rows.push(HistogramRow { stars, percent });
    }
    rows
}

fn parse_review(el: ElementRef) -> Review {
    Review {
        id: el
            .attr("id")
            .filter(|id| id.starts_with('R'))
            .or_else(|| el.select(&REPORT).next().and_then(|e| e.attr("data-id")))
            .map(str::to_string),
        author: first_text(el, &PROFILE_NAME),
        rating: first_text(el, &REVIEW_STARS).and_then(|t| parse_rating(&t)),
        title: first_text(el, &REVIEW_TITLE).map(|t| strip_star_prefix(&t)),
        date: first_text(el, &REVIEW_DATE),
        verified: el.select(&AVP).next().is_some(),
        variant: first_text(el, &VARIANT),
        helpful: first_text(el, &HELPFUL),
        body: first_text(el, &REVIEW_BODY).unwrap_or_default(),
    }
}

fn text(el: ElementRef) -> String {
    let mut out = String::new();
    for node in el.descendants() {
        let Some(t) = node.value().as_text() else {
            continue;
        };
        let in_code = node
            .parent()
            .and_then(|p| p.value().as_element().map(|e| e.name()))
            .is_some_and(|n| matches!(n, "script" | "style" | "noscript"));
        if in_code {
            continue;
        }
        for word in t.split_whitespace() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(word);
        }
    }
    out
}

fn first_text(el: ElementRef, sel: &Selector) -> Option<String> {
    el.select(sel).map(text).find(|t| !t.is_empty())
}

fn dedup(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in items {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn strip_label(t: &str) -> String {
    let start = t.find(['￥', '¥', '$']).unwrap_or(0);
    t[start..].trim().to_string()
}

fn strip_star_prefix(t: &str) -> String {
    for marker in ["stars ", "つ "] {
        if let Some(i) = t.find(marker)
            && i < 40
        {
            return t[i + marker.len()..].trim().to_string();
        }
    }
    t.to_string()
}

fn clean_availability(t: &str) -> String {
    let t = t.split("在庫状況 について").next().unwrap_or(t);
    t.trim().to_string()
}

pub fn parse_money(t: &str) -> Option<f64> {
    let digits: String = t
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .filter(|c| *c != ',')
        .collect();
    digits.parse().ok()
}

pub fn parse_rating(t: &str) -> Option<f64> {
    let tail = if let Some(i) = t.find("のうち") {
        &t[i + "のうち".len()..]
    } else if let Some(i) = t.find('中') {
        &t[i + '中'.len_utf8()..]
    } else {
        t
    };
    let n: String = tail
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    n.parse().ok().filter(|r| (0.0..=5.0).contains(r))
}

pub fn parse_count(t: &str) -> Option<u64> {
    let start = t.find(|c: char| c.is_ascii_digit())?;
    let rest = &t[start..];
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .filter(|c| *c != ',')
        .collect();
    let after = rest[num.len()..].trim_start();
    let value: f64 = num.parse().ok()?;
    let mult = match after.chars().next() {
        Some('K' | 'k') => 1_000.0,
        Some('M') => 1_000_000.0,
        Some('万') => 10_000.0,
        _ => 1.0,
    };
    Some((value * mult).round() as u64)
}

fn number_before(t: &str, marker: char) -> Option<u8> {
    let end = t.find(marker)?;
    let digits: String = t[..end]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_skips_scripts() {
        let doc = Html::parse_document(
            r#"<div id="availability"><span>在庫あり。</span><script>{"isRobot":false}</script><style>.x{}</style></div>"#,
        );
        let sel = Selector::parse("#availability").unwrap();
        assert_eq!(text(doc.select(&sel).next().unwrap()), "在庫あり。");
    }

    #[test]
    fn ratings() {
        assert_eq!(parse_rating("4.3 out of 5 stars"), Some(4.3));
        assert_eq!(parse_rating("5つ星のうち4.4"), Some(4.4));
        assert_eq!(parse_rating("星5つ中5つ"), Some(5.0));
        assert_eq!(parse_rating("5.0 out of 5 stars"), Some(5.0));
    }

    #[test]
    fn counts() {
        assert_eq!(parse_count("46,510 レビュー"), Some(46510));
        assert_eq!(parse_count("(5.9K)"), Some(5900));
        assert_eq!(parse_count("600 レーティング"), Some(600));
    }

    #[test]
    fn money() {
        assert_eq!(parse_money("￥1,963"), Some(1963.0));
        assert_eq!(parse_money("$12.99"), Some(12.99));
        assert_eq!(parse_money("過去: ￥3,999"), Some(3999.0));
    }

    #[test]
    fn asins() {
        assert_eq!(parse_asin("b0ct857v89"), Some(("B0CT857V89".into(), None)));
        assert_eq!(
            parse_asin("https://www.amazon.co.jp/AOKIMI-x/dp/B0CT857V89/ref=sr_1_1?k=v"),
            Some(("B0CT857V89".into(), Some(Market::Jp)))
        );
        assert_eq!(
            parse_asin("https://www.amazon.com/gp/product/B07V78HMDQ"),
            Some(("B07V78HMDQ".into(), Some(Market::Com)))
        );
        assert_eq!(parse_asin("https://example.com/foo"), None);
    }

    #[test]
    fn search_urls() {
        assert_eq!(
            search_url(Market::Jp, "イヤホン 白", 2, Sort::PriceAsc),
            "https://www.amazon.co.jp/s?k=%E3%82%A4%E3%83%A4%E3%83%9B%E3%83%B3+%E7%99%BD&page=2&s=price-asc-rank"
        );
    }
}

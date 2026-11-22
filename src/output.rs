use anstream::println;
use anstyle::{AnsiColor, Style};
use shop::{amazon, product, rakuten, venue};

const PRICE: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)));
const LIST_PRICE: Style = Style::new().dimmed().strikethrough();
const RATING: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Yellow)));
const BADGE: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));
const WARN: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Magenta)));
const TITLE: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
pub const ERROR: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)));

fn paint(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

fn label(name: &str) -> String {
    paint(DIM, &format!("{name:<13}"))
}

fn stars(rating: Option<f64>, count: Option<u64>) -> String {
    stars_with(1, rating, count)
}

fn venue_stars(rating: Option<f64>, count: Option<u64>) -> String {
    stars_with(2, rating, count)
}

fn stars_with(decimals: usize, rating: Option<f64>, count: Option<u64>) -> String {
    let text = match (rating, count) {
        (Some(r), Some(c)) => format!("★{r:.decimals$} ({c})"),
        (Some(r), None) => format!("★{r:.decimals$}"),
        (None, Some(c)) => format!("({c} ratings)"),
        (None, None) => return paint(DIM, "no rating"),
    };
    paint(RATING, &text)
}

pub fn search(results: &[amazon::SearchResult]) {
    for (i, r) in results.iter().enumerate() {
        let mut head = format!(
            "{:>2}. {}  {}",
            i + 1,
            match &r.price {
                Some(p) => paint(PRICE, p),
                None => paint(DIM, "no price"),
            },
            stars(r.rating, r.rating_count)
        );
        if let Some(lp) = &r.list_price {
            head.push_str(&format!("  {}", paint(LIST_PRICE, lp)));
        }
        if r.sponsored {
            head.push_str(&format!("  {}", paint(WARN, "[sponsored]")));
        }
        for b in &r.badges {
            head.push_str(&format!("  {}", paint(BADGE, &format!("[{b}]"))));
        }
        println!("{head}");
        println!("    {}", paint(TITLE, &r.title));
        if let Some(b) = &r.bought_past_month {
            println!("    {b}");
        }
        println!("    {}", paint(DIM, &r.url));
    }
}

pub fn item(item: &amazon::Item) {
    println!("{}", paint(TITLE, &item.title));
    println!("{}", paint(DIM, &item.url));
    if let Some(v) = &item.brand {
        println!("{}{v}", label("brand"));
    }
    if let Some(v) = &item.price {
        println!("{}{}", label("price"), paint(PRICE, v));
    }
    if let Some(v) = &item.list_price {
        println!("{}{}", label("list price"), paint(LIST_PRICE, v));
    }
    if let Some(v) = &item.availability {
        println!("{}{v}", label("availability"));
    }
    if let Some(v) = &item.seller {
        println!("{}{v}", label("seller"));
    }
    println!(
        "{}{}",
        label("rating"),
        stars(item.rating, item.rating_count)
    );
    if !item.histogram.is_empty() {
        let h: Vec<String> = item
            .histogram
            .iter()
            .map(|r| format!("{}★ {}%", r.stars, r.percent))
            .collect();
        println!("{}{}", label(""), paint(RATING, &h.join("  ")));
    }
    if !item.categories.is_empty() {
        println!("{}{}", label("category"), item.categories.join(" › "));
    }
    if !item.features.is_empty() {
        println!("\n{}", paint(TITLE, "features"));
        for f in &item.features {
            println!("  - {f}");
        }
    }
    if !item.details.is_empty() {
        println!("\n{}", paint(TITLE, "details"));
        for (k, v) in &item.details {
            println!("  {}: {v}", paint(DIM, k));
        }
    }
    if !item.reviews.is_empty() {
        println!(
            "\n{}",
            paint(
                DIM,
                &format!(
                    "{} top reviews (shop amazon reviews {})",
                    item.reviews.len(),
                    item.asin
                )
            )
        );
    }
}

pub fn reviews(item: &amazon::Item) {
    println!(
        "{}  {}",
        paint(TITLE, &item.title),
        stars(item.rating, item.rating_count)
    );
    for r in &item.reviews {
        println!();
        let rating = r
            .rating
            .map(|x| paint(RATING, &format!("★{x:.0}")))
            .unwrap_or_default();
        let verified = if r.verified {
            format!("  {}", paint(BADGE, "[verified purchase]"))
        } else {
            String::new()
        };
        println!(
            "{rating} {}{verified}",
            paint(TITLE, r.title.as_deref().unwrap_or(""))
        );
        let by: Vec<&str> = [r.author.as_deref(), r.date.as_deref(), r.variant.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        println!("{}", paint(DIM, &by.join(" · ")));
        println!("{}", r.body);
        if let Some(h) = &r.helpful {
            println!("{}", paint(DIM, &format!("({h})")));
        }
    }
}

pub fn rakuten_search(res: &rakuten::SearchPage) {
    println!(
        "{}",
        paint(
            DIM,
            &format!(
                "{} results, page {}/{}",
                res.count, res.page, res.page_count
            )
        )
    );
    for (i, it) in res.items.iter().enumerate() {
        let mut head = format!(
            "{:>2}. {}  {}  {}",
            i + 1,
            paint(PRICE, &yen(it.price)),
            paint(RATING, &format!("★{:.2} ({})", it.rating, it.rating_count)),
            it.shop
        );
        if it.postage_included {
            head.push_str(&format!("  {}", paint(BADGE, "[送料込]")));
        }
        if !it.in_stock {
            head.push_str(&format!("  {}", paint(WARN, "[out of stock]")));
        }
        println!("{head}");
        println!("    {}", paint(TITLE, &it.name));
        println!(
            "    {}",
            paint(DIM, &format!("{}  ({})", it.url, it.item_code))
        );
    }
}

pub fn rakuten_item(it: &rakuten::Item) {
    println!("{}", paint(TITLE, &it.name));
    println!("{}", paint(DIM, &it.url));
    println!("{}{}", label("price"), paint(PRICE, &yen(it.price)));
    println!("{}{} ({})", label("shop"), it.shop, it.shop_code);
    println!(
        "{}{}",
        label("rating"),
        paint(RATING, &format!("★{:.2} ({})", it.rating, it.rating_count))
    );
    let stock = if it.in_stock {
        "yes".to_string()
    } else {
        paint(WARN, "no")
    };
    println!("{}{stock}", label("in stock"));
    println!(
        "{}{}",
        label("postage incl."),
        if it.postage_included { "yes" } else { "no" }
    );
    println!("{}{}x", label("points"), it.point_rate);
    if let Some(c) = &it.catchcopy {
        println!("\n{c}");
    }
    if let Some(c) = &it.caption {
        println!("\n{c}");
    }
}

pub fn products(res: &product::SearchPage) {
    if let Some(total) = res.total {
        println!(
            "{}",
            paint(DIM, &format!("{total} results, page {}", res.page))
        );
    }
    for (i, p) in res.items.iter().enumerate() {
        let mut head = format!("{:>2}. {}", i + 1, price(p.price));
        if p.rating.is_some() || p.rating_count.is_some() {
            head.push_str(&format!("  {}", stars(p.rating, p.rating_count)));
        }
        if let Some(lp) = p.list_price.filter(|lp| Some(*lp) != p.price) {
            head.push_str(&format!("  {}", paint(LIST_PRICE, &yen(lp))));
        }
        if p.in_stock == Some(false) {
            head.push_str(&format!("  {}", paint(WARN, "[out of stock]")));
        }
        for b in &p.badges {
            head.push_str(&format!("  {}", paint(BADGE, &format!("[{b}]"))));
        }
        println!("{head}");
        let name = match p.brand.as_ref().filter(|b| !p.name.starts_with(b.as_str())) {
            Some(b) => format!("{} {}", paint(DIM, b), paint(TITLE, &p.name)),
            None => paint(TITLE, &p.name),
        };
        println!("    {name}");
        println!("    {}", paint(DIM, &p.url));
    }
}

pub fn product(p: &product::Product) {
    println!("{}", paint(TITLE, &p.name));
    println!("{}", paint(DIM, &p.url));
    println!("{}{}", label("id"), p.id);
    if let Some(v) = &p.brand {
        println!("{}{v}", label("brand"));
    }
    println!("{}{}", label("price"), price(p.price));
    if let Some(lp) = p.list_price.filter(|lp| Some(*lp) != p.price) {
        println!("{}{}", label("list price"), paint(LIST_PRICE, &yen(lp)));
    }
    if p.rating.is_some() || p.rating_count.is_some() {
        println!("{}{}", label("rating"), stars(p.rating, p.rating_count));
    }
    if let Some(s) = p.in_stock {
        let v = if s {
            "yes".to_string()
        } else {
            paint(WARN, "no")
        };
        println!("{}{v}", label("in stock"));
    }
    if !p.badges.is_empty() {
        println!("{}{}", label("badges"), paint(BADGE, &p.badges.join(", ")));
    }
    if !p.categories.is_empty() {
        println!("{}{}", label("category"), p.categories.join(" › "));
    }
    if !p.variants.is_empty() {
        println!("{}{}", label("variants"), p.variants.join(", "));
    }
    if !p.details.is_empty() {
        println!("\n{}", paint(TITLE, "details"));
        for (k, v) in &p.details {
            println!("  {}: {v}", paint(DIM, k));
        }
    }
    if let Some(d) = &p.description {
        println!("\n{d}");
    }
}

pub fn venues(res: &venue::SearchPage) {
    if let Some(total) = res.total {
        println!(
            "{}",
            paint(DIM, &format!("{total} results, page {}", res.page))
        );
    }
    for (i, v) in res.items.iter().enumerate() {
        let mut head = Vec::new();
        if v.rating.is_some() || v.rating_count.is_some() {
            head.push(venue_stars(v.rating, v.rating_count));
        }
        if let Some(b) = &v.budget {
            head.push(paint(PRICE, b));
        }
        for b in &v.badges {
            head.push(paint(BADGE, &format!("[{b}]")));
        }
        println!("{:>2}. {}", i + 1, head.join("  "));
        let mut name = paint(TITLE, &v.name);
        if !v.genre.is_empty() {
            name.push_str(&format!("  {}", paint(DIM, &v.genre.join(", "))));
        }
        println!("    {name}");
        if let Some(a) = v.access.as_ref().or(v.area.as_ref()) {
            println!("    {a}");
        }
        println!("    {}", paint(DIM, &v.url));
    }
}

pub fn venue(v: &venue::Venue) {
    println!("{}", paint(TITLE, &v.name));
    println!("{}", paint(DIM, &v.url));
    println!("{}{}", label("id"), v.id);
    if !v.genre.is_empty() {
        println!("{}{}", label("genre"), v.genre.join(", "));
    }
    if v.rating.is_some() || v.rating_count.is_some() {
        println!(
            "{}{}",
            label("rating"),
            venue_stars(v.rating, v.rating_count)
        );
    }
    let rows = [
        ("budget", &v.budget),
        ("area", &v.area),
        ("address", &v.address),
        ("access", &v.access),
        ("hours", &v.hours),
        ("closed", &v.closed),
        ("phone", &v.phone),
        ("booking", &v.booking_url),
    ];
    for (name, value) in rows {
        if let Some(value) = value {
            let mut lines = value.lines();
            println!("{}{}", label(name), lines.next().unwrap_or_default());
            for l in lines {
                println!("{}{l}", label(""));
            }
        }
    }
    if !v.badges.is_empty() {
        println!("{}{}", label("badges"), paint(BADGE, &v.badges.join(", ")));
    }
    if !v.details.is_empty() {
        println!("\n{}", paint(TITLE, "details"));
        for (k, val) in &v.details {
            println!("  {}: {val}", paint(DIM, k));
        }
    }
    if !v.menus.is_empty() {
        println!("\n{}", paint(TITLE, "menus"));
        for m in &v.menus {
            let p = match (m.price, &m.price_text) {
                (Some(p), _) => paint(PRICE, &yen(p)),
                (None, Some(t)) => paint(PRICE, t),
                (None, None) => String::new(),
            };
            println!("  {p}  {}", m.name);
            if let Some(d) = &m.description {
                println!("    {}", paint(DIM, d));
            }
        }
    }
    if let Some(d) = &v.description {
        println!("\n{d}");
    }
}

fn price(p: Option<u64>) -> String {
    match p {
        Some(p) => paint(PRICE, &yen(p)),
        None => paint(DIM, "no price"),
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn yen() {
        assert_eq!(super::yen(4980), "￥4,980");
        assert_eq!(super::yen(1234567), "￥1,234,567");
        assert_eq!(super::yen(980), "￥980");
    }

    #[test]
    fn paint_wraps_in_ansi() {
        let s = super::paint(super::PRICE, "￥1");
        assert!(s.starts_with("\x1b[") && s.ends_with("\x1b[0m"));
        assert_eq!(anstream::adapter::strip_str(&s).to_string(), "￥1");
    }
}

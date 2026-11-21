use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use shop::{
    amazon::{self, Market},
    fetch::{FetchError, FetchMode, Fetcher},
    rakuten,
};

#[derive(Parser)]
#[command(
    version,
    about = "Search and inspect products on Amazon (.co.jp, .com) and Rakuten Ichiba"
)]
struct Cli {
    #[arg(long, global = true, help = "Print JSON instead of text")]
    json: bool,
    #[arg(short, long, global = true, help = "Log requests to stderr")]
    verbose: bool,
    #[command(subcommand)]
    store: Store,
}

#[derive(Subcommand)]
enum Store {
    #[command(
        about = "Amazon (scrapes public pages; full review lists need a login and are not supported)"
    )]
    Amazon {
        #[arg(long, short, global = true, value_enum, default_value = "jp")]
        market: Market,
        #[arg(
            long,
            global = true,
            value_enum,
            default_value = "auto",
            help = "auto: plain HTTP, headless Chrome when blocked"
        )]
        fetch: FetchMode,
        #[command(subcommand)]
        cmd: AmazonCmd,
    },
    #[command(about = "Rakuten Ichiba (official API; needs RAKUTEN_APP_ID and RAKUTEN_ACCESS_KEY)")]
    Rakuten {
        #[command(subcommand)]
        cmd: RakutenCmd,
    },
}

#[derive(Subcommand)]
enum AmazonCmd {
    #[command(about = "Search products")]
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, value_enum, default_value = "relevance")]
        sort: amazon::Sort,
        #[arg(long, help = "Drop sponsored results")]
        no_ads: bool,
        #[arg(long, short = 'n', help = "Show at most N results")]
        limit: Option<usize>,
    },
    #[command(about = "Product details: price, seller, rating histogram, specs, top reviews")]
    Item {
        #[arg(help = "ASIN or product URL")]
        asin: String,
    },
    #[command(about = "Top reviews shown on the product page")]
    Reviews {
        #[arg(help = "ASIN or product URL")]
        asin: String,
    },
}

#[derive(Subcommand)]
enum RakutenCmd {
    #[command(about = "Search items")]
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(
            long,
            short = 'n',
            default_value_t = 30,
            help = "Results per page (max 30)"
        )]
        limit: u32,
        #[arg(long, value_enum)]
        sort: Option<rakuten::Sort>,
        #[arg(long)]
        min_price: Option<u64>,
        #[arg(long)]
        max_price: Option<u64>,
        #[arg(long, help = "Only items in stock")]
        in_stock: bool,
    },
    #[command(about = "Look up one item by item code (shop:itemid)")]
    Item { item_code: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shop: {e:#}");
            match e.downcast_ref::<FetchError>() {
                Some(FetchError::Blocked(_)) => ExitCode::from(3),
                Some(FetchError::NotFound(_)) => ExitCode::from(4),
                None => ExitCode::FAILURE,
            }
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let json = cli.json;
    match cli.store {
        Store::Amazon { market, fetch, cmd } => {
            let fetcher = Fetcher::new(fetch, cli.verbose);
            match cmd {
                AmazonCmd::Search {
                    query,
                    page,
                    sort,
                    no_ads,
                    limit,
                } => {
                    let query = join_query(&query)?;
                    let html =
                        fetcher.amazon(&amazon::search_url(market, &query, page, sort), market)?;
                    let mut results = amazon::parse_search(&html, market);
                    if no_ads {
                        results.retain(|r| !r.sponsored);
                    }
                    if let Some(n) = limit {
                        results.truncate(n);
                    }
                    if results.is_empty() && !html.contains("s-search-result") {
                        bail!(
                            "no search results found on the page (layout changed or empty result)"
                        );
                    }
                    emit(json, &results, || print_search(&results))
                }
                AmazonCmd::Item { asin } => {
                    let item = fetch_item(&fetcher, market, &asin)?;
                    emit(json, &item, || print_item(&item))
                }
                AmazonCmd::Reviews { asin } => {
                    let item = fetch_item(&fetcher, market, &asin)?;
                    emit(json, &item.reviews, || print_reviews(&item))
                }
            }
        }
        Store::Rakuten { cmd } => {
            let fetcher = Fetcher::new(FetchMode::Http, cli.verbose);
            let creds = rakuten_credentials()?;
            match cmd {
                RakutenCmd::Search {
                    query,
                    page,
                    limit,
                    sort,
                    min_price,
                    max_price,
                    in_stock,
                } => {
                    let query = join_query(&query)?;
                    let q = rakuten::Query {
                        keyword: Some(&query),
                        page,
                        hits: limit,
                        sort,
                        min_price,
                        max_price,
                        in_stock,
                        ..Default::default()
                    };
                    let res = rakuten::search(&fetcher, &creds, &q)?;
                    emit(json, &res, || print_rakuten_search(&res))
                }
                RakutenCmd::Item { item_code } => {
                    let q = rakuten::Query {
                        item_code: Some(&item_code),
                        hits: 1,
                        ..Default::default()
                    };
                    let res = rakuten::search(&fetcher, &creds, &q)?;
                    let item = res
                        .items
                        .into_iter()
                        .next()
                        .with_context(|| format!("no Rakuten item with code {item_code}"))?;
                    emit(json, &item, || print_rakuten_item(&item))
                }
            }
        }
    }
}

fn join_query(parts: &[String]) -> Result<String> {
    let q = parts.join(" ");
    if q.trim().is_empty() {
        bail!("missing search query");
    }
    Ok(q)
}

fn fetch_item(fetcher: &Fetcher, market: Market, input: &str) -> Result<amazon::Item> {
    let (asin, url_market) = amazon::parse_asin(input)
        .with_context(|| format!("not an ASIN or Amazon product URL: {input}"))?;
    let market = url_market.unwrap_or(market);
    let html = fetcher.amazon(&market.product_url(&asin), market)?;
    let item = amazon::parse_item(&html, market, &asin);
    if item.title.is_empty() {
        bail!("could not find a product title on the page (layout changed?)");
    }
    Ok(item)
}

fn rakuten_credentials() -> Result<rakuten::Credentials> {
    let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let (Some(app_id), Some(access_key)) = (get("RAKUTEN_APP_ID"), get("RAKUTEN_ACCESS_KEY"))
    else {
        bail!(
            "set RAKUTEN_APP_ID and RAKUTEN_ACCESS_KEY (free: register an app at https://webservice.rakuten.co.jp/app/create)"
        );
    };
    Ok(rakuten::Credentials {
        app_id,
        access_key,
        referer: get("RAKUTEN_REFERER"),
    })
}

fn emit<T: Serialize>(json: bool, value: &T, text: impl FnOnce()) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        text();
    }
    Ok(())
}

fn stars(rating: Option<f64>, count: Option<u64>) -> String {
    match (rating, count) {
        (Some(r), Some(c)) => format!("★{r:.1} ({c})"),
        (Some(r), None) => format!("★{r:.1}"),
        (None, Some(c)) => format!("({c} ratings)"),
        (None, None) => "no rating".into(),
    }
}

fn print_search(results: &[amazon::SearchResult]) {
    for (i, r) in results.iter().enumerate() {
        let mut head = format!(
            "{:>2}. {}  {}",
            i + 1,
            r.price.as_deref().unwrap_or("no price"),
            stars(r.rating, r.rating_count)
        );
        if let Some(lp) = &r.list_price {
            head.push_str(&format!("  list {lp}"));
        }
        if r.sponsored {
            head.push_str("  [sponsored]");
        }
        for b in &r.badges {
            head.push_str(&format!("  [{b}]"));
        }
        println!("{head}");
        println!("    {}", r.title);
        if let Some(b) = &r.bought_past_month {
            println!("    {b}");
        }
        println!("    {}", r.url);
    }
}

fn print_item(item: &amazon::Item) {
    println!("{}", item.title);
    println!("{}", item.url);
    let field = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            println!("{k:<13}{v}");
        }
    };
    field("brand", &item.brand);
    field("price", &item.price);
    field("list price", &item.list_price);
    field("availability", &item.availability);
    field("seller", &item.seller);
    println!("{:<13}{}", "rating", stars(item.rating, item.rating_count));
    if !item.histogram.is_empty() {
        let h: Vec<String> = item
            .histogram
            .iter()
            .map(|r| format!("{}★ {}%", r.stars, r.percent))
            .collect();
        println!("{:<13}{}", "", h.join("  "));
    }
    if !item.categories.is_empty() {
        println!("{:<13}{}", "category", item.categories.join(" › "));
    }
    if !item.features.is_empty() {
        println!("\nfeatures:");
        for f in &item.features {
            println!("  - {f}");
        }
    }
    if !item.details.is_empty() {
        println!("\ndetails:");
        for (k, v) in &item.details {
            println!("  {k}: {v}");
        }
    }
    if !item.reviews.is_empty() {
        println!(
            "\n{} top reviews (shop amazon reviews {})",
            item.reviews.len(),
            item.asin
        );
    }
}

fn print_reviews(item: &amazon::Item) {
    println!("{}  {}", item.title, stars(item.rating, item.rating_count));
    for r in &item.reviews {
        println!();
        let rating = r.rating.map(|x| format!("★{x:.0}")).unwrap_or_default();
        println!(
            "{rating} {}{}",
            r.title.as_deref().unwrap_or(""),
            if r.verified {
                "  [verified purchase]"
            } else {
                ""
            }
        );
        let by: Vec<&str> = [r.author.as_deref(), r.date.as_deref(), r.variant.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        println!("{}", by.join(" · "));
        println!("{}", r.body);
        if let Some(h) = &r.helpful {
            println!("({h})");
        }
    }
}

fn print_rakuten_search(res: &rakuten::SearchPage) {
    println!(
        "{} results, page {}/{}",
        res.count, res.page, res.page_count
    );
    for (i, it) in res.items.iter().enumerate() {
        let mut head = format!(
            "{:>2}. ¥{}  ★{:.2} ({})  {}",
            i + 1,
            it.price,
            it.rating,
            it.rating_count,
            it.shop
        );
        if it.postage_included {
            head.push_str("  [送料込]");
        }
        if !it.in_stock {
            head.push_str("  [out of stock]");
        }
        println!("{head}");
        println!("    {}", it.name);
        println!("    {}  ({})", it.url, it.item_code);
    }
}

fn print_rakuten_item(it: &rakuten::Item) {
    println!("{}", it.name);
    println!("{}", it.url);
    println!("{:<13}¥{}", "price", it.price);
    println!("{:<13}{} ({})", "shop", it.shop, it.shop_code);
    println!("{:<13}★{:.2} ({})", "rating", it.rating, it.rating_count);
    println!("{:<13}{}", "in stock", it.in_stock);
    println!("{:<13}{}", "postage incl.", it.postage_included);
    println!("{:<13}{}x", "points", it.point_rate);
    if let Some(c) = &it.catchcopy {
        println!("\n{c}");
    }
    if let Some(c) = &it.caption {
        println!("\n{c}");
    }
}

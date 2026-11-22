use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde::Serialize;
use shop::{
    amazon::{self, Market},
    fetch::{FetchError, Fetcher},
    iherb, ikea, muji, nitori,
    product::{self, Connector},
    rakuten, uniqlo,
};

mod output;
mod update;

#[derive(Parser)]
#[command(
    version,
    about = "Searches Japanese shops with pure HTTP calls. Connectors: amazon, rakuten, iherb, ikea, nitori, muji, uniqlo"
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
        #[command(subcommand)]
        cmd: AmazonCmd,
    },
    #[command(about = "Rakuten Ichiba (official API; needs RAKUTEN_APP_ID and RAKUTEN_ACCESS_KEY)")]
    Rakuten {
        #[command(subcommand)]
        cmd: RakutenCmd,
    },
    #[command(about = "iHerb Japan (jp.iherb.com)")]
    Iherb {
        #[command(subcommand)]
        cmd: ShopCmd,
    },
    #[command(about = "IKEA Japan (ikea.com/jp)")]
    Ikea {
        #[command(subcommand)]
        cmd: ShopCmd,
    },
    #[command(about = "Nitori (nitori-net.jp)")]
    Nitori {
        #[command(subcommand)]
        cmd: ShopCmd,
    },
    #[command(about = "Muji Japan (muji.com/jp)")]
    Muji {
        #[command(subcommand)]
        cmd: ShopCmd,
    },
    #[command(about = "Uniqlo Japan (uniqlo.com/jp)")]
    Uniqlo {
        #[command(subcommand)]
        cmd: ShopCmd,
    },
    #[command(
        about = "Print a shell completion script",
        long_about = "Print a shell completion script.\n\n  bash: shop completions bash > ~/.local/share/bash-completion/completions/shop\n  zsh:  shop completions zsh > \"${fpath[1]}/_shop\"\n  fish: shop completions fish > ~/.config/fish/completions/shop.fish"
    )]
    Completions { shell: Shell },
    #[command(about = "Update shop to the latest GitHub release (checksum verified)")]
    Update {
        #[arg(long, help = "Only report whether a newer release exists")]
        check: bool,
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

#[derive(Subcommand)]
enum ShopCmd {
    #[command(about = "Search products")]
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, short = 'n', help = "Show at most N results")]
        limit: Option<u32>,
        #[arg(long, value_enum, default_value = "relevance")]
        sort: product::Sort,
    },
    #[command(about = "Product details")]
    Item {
        #[arg(help = "Product id or URL")]
        id: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let notify =
        !cli.json && !matches!(cli.store, Store::Completions { .. } | Store::Update { .. });
    let result = run(cli);
    if notify {
        update::notice();
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let red = output::ERROR;
            anstream::eprintln!("{red}shop:{red:#} {e:#}");
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
        Store::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        Store::Update { check } => update::run(check),
        Store::Amazon { market, cmd } => {
            let fetcher = Fetcher::new(cli.verbose);
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
                    emit(json, &results, || output::search(&results))
                }
                AmazonCmd::Item { asin } => {
                    let item = fetch_item(&fetcher, market, &asin)?;
                    emit(json, &item, || output::item(&item))
                }
                AmazonCmd::Reviews { asin } => {
                    let item = fetch_item(&fetcher, market, &asin)?;
                    emit(json, &item.reviews, || output::reviews(&item))
                }
            }
        }
        Store::Iherb { cmd } => run_shop(&iherb::Iherb, cmd, json, cli.verbose),
        Store::Ikea { cmd } => run_shop(&ikea::Ikea, cmd, json, cli.verbose),
        Store::Nitori { cmd } => run_shop(&nitori::Nitori, cmd, json, cli.verbose),
        Store::Muji { cmd } => run_shop(&muji::Muji, cmd, json, cli.verbose),
        Store::Uniqlo { cmd } => run_shop(&uniqlo::Uniqlo, cmd, json, cli.verbose),
        Store::Rakuten { cmd } => {
            let fetcher = Fetcher::new(cli.verbose);
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
                    emit(json, &res, || output::rakuten_search(&res))
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
                    emit(json, &item, || output::rakuten_item(&item))
                }
            }
        }
    }
}

fn run_shop(shop: &dyn Connector, cmd: ShopCmd, json: bool, verbose: bool) -> Result<()> {
    let fetcher = Fetcher::new(verbose);
    match cmd {
        ShopCmd::Search {
            query,
            page,
            limit,
            sort,
        } => {
            let text = join_query(&query)?;
            let q = product::Query {
                text: &text,
                page,
                limit,
                sort,
            };
            let mut res = shop.search(&fetcher, &q)?;
            if let Some(n) = limit {
                res.items.truncate(n as usize);
            }
            emit(json, &res, || output::products(&res))
        }
        ShopCmd::Item { id } => {
            let p = shop.item(&fetcher, &id)?;
            emit(json, &p, || output::product(&p))
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

# Reference

## Commands

| Command | Output |
|---|---|
| `shop amazon search <query>` | ASIN, title, URL, price, list price, rating, rating count, "bought in past month", sponsored flag, badges, delivery, image |
| `shop amazon item <asin\|url>` | Title, brand, price, list price, availability, seller, rating, rating count, histogram, categories, bullet points, specification rows, image, top reviews |
| `shop amazon reviews <asin\|url>` | Top reviews: id, author, rating, title, date, verified purchase, variant, helpful votes, body |
| `shop rakuten search <query>` | Total count, page, page count, items: code, name, URL, price, shop, rating, rating count, stock, postage included, point rate, catch copy, caption, images |
| `shop rakuten item <shop:itemid>` | One item, same fields |
| `shop <iherb\|ikea\|nitori\|uniqlo> search <query>` | Total count, page, items: id, name, URL, brand, price, list price, rating, rating count, stock, badges, variants, image |
| `shop <iherb\|ikea\|nitori\|uniqlo> item <id\|url>` | Same fields plus categories, specification rows (`details`) and description |
| `shop completions <bash\|zsh\|fish\|elvish\|powershell>` | Completion script on stdout |
| `shop update [--check]` | Installs the latest GitHub release after checking its SHA-256; `--check` only reports |

## Options

| Option | Scope | Effect |
|---|---|---|
| `--json` | all | JSON on stdout instead of text |
| `-v`, `--verbose` | all | Requests and HTTP status on stderr |
| `-m`, `--market jp\|com` | amazon | Marketplace, default `jp`. A product URL selects its own marketplace |
| `--page N` | search | Result page |
| `--sort` | amazon search | `relevance`, `price-asc`, `price-desc`, `reviews`, `newest` |
| `--no-ads` | amazon search | Drop sponsored results |
| `-n N` | search | Amazon, iHerb, IKEA, Nitori, Uniqlo: keep the first N results. Rakuten: results per page, max 30 |
| `--sort` | rakuten search | `standard`, `price-asc`, `price-desc`, `reviews`, `rating`, `newest` |
| `--min-price`, `--max-price` | rakuten search | Price range in yen |
| `--sort` | iherb, ikea, nitori, uniqlo search | `relevance`, `price-asc`, `price-desc`, `reviews`, `newest` |
| `--in-stock` | rakuten search | Only items in stock |

## Output

Text output is colored when stdout is a terminal. `NO_COLOR=1`, a pipe or `--json` turn color off.

| Variable | Effect |
|---|---|
| `NO_COLOR` | Disables color |
| `SHOP_NO_UPDATE_CHECK=1` | Disables the daily "newer release available" notice |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Error: bad input, API error, page layout not recognised |
| 3 | Blocked: captcha, 503 or challenge page |
| 4 | Page not found, or a refusal disguised as one |

## Rakuten API

Endpoint `https://openapi.rakuten.co.jp/ichibams/api/IchibaItem/Search/20260701`, `formatVersion=2`. Limits: about one request per second per app, 30 items per page, 3,000 results per query.

## Shop endpoints

| Connector | Search | Item | IDs accepted |
|---|---|---|---|
| `iherb` | `jp.iherb.com/search` HTML, 48 per page | `catalog.app.iherb.com/product/<id>` JSON | `70316`, `jp.iherb.com/pr/<slug>/<id>` |
| `ikea` | `sik.search.blue.cdtapps.com/jp/ja/search-result-page` JSON, up to 500 results | `ikea.com/jp/ja/p/-<id>/` JSON-LD and HTML, stock from `api.salesitem.ingka.com` | `40432112`, `404.321.12`, `s49434096`, product URL |
| `nitori` | `nitori-net.jp/occ/v2/nitorinet/nitori/products/search` JSON | `.../products/<code>` JSON | `8761161s`, `nitori-net.jp/ec/product/<code>/` |
| `uniqlo` | `uniqlo.com/jp/api/commerce/v5/ja/products` JSON, up to 100 per page | `.../products/<id>/price-groups/<pg>/details` and `/l2s` stock JSON | `E465185-000`, `465185`, product URL |

Prices are integers in yen. `price` is what the shop charges now; `list_price` is set only when the shop shows a higher regular price.

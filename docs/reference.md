# Reference

## Commands

| Command | Output |
|---|---|
| `shop amazon search <query>` | ASIN, title, URL, price, list price, rating, rating count, "bought in past month", sponsored flag, badges, delivery, image |
| `shop amazon item <asin\|url>` | Title, brand, price, list price, availability, seller, rating, rating count, histogram, categories, bullet points, specification rows, image, top reviews |
| `shop amazon reviews <asin\|url>` | Top reviews: id, author, rating, title, date, verified purchase, variant, helpful votes, body |
| `shop rakuten search <query>` | Total count, page, page count, items: code, name, URL, price, shop, rating, rating count, stock, postage included, point rate, catch copy, caption, images |
| `shop rakuten item <shop:itemid>` | One item, same fields |
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
| `-n N` | search | Amazon: keep the first N results. Rakuten: results per page, max 30 |
| `--sort` | rakuten search | `standard`, `price-asc`, `price-desc`, `reviews`, `rating`, `newest` |
| `--min-price`, `--max-price` | rakuten search | Price range in yen |
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
| 3 | Blocked by Amazon: captcha, 503 or challenge page |
| 4 | Page not found, or a refusal disguised as one |

## Rakuten API

Endpoint `https://openapi.rakuten.co.jp/ichibams/api/IchibaItem/Search/20260701`, `formatVersion=2`. Limits: about one request per second per app, 30 items per page, 3,000 results per query.

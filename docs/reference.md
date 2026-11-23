# Reference

## Commands

| Command | Output |
|---|---|
| `shop amazon search <query>` | ASIN, title, URL, price, list price, rating, rating count, "bought in past month", sponsored flag, badges, delivery, image |
| `shop amazon item <asin\|url>` | Title, brand, price, list price, availability, seller, rating, rating count, histogram, categories, bullet points, specification rows, image, top reviews |
| `shop amazon reviews <asin\|url>` | Top reviews: id, author, rating, title, date, verified purchase, variant, helpful votes, body |
| `shop amazon price <asin\|url>` | Writes Keepa's price history chart (Amazon and new offers) as a PNG; prints the path and the Keepa product page. JSON: asin, market, range, path, chart URL, Keepa URL |
| `shop rakuten search <query>` | Total count, page, page count, items: code, name, URL, price, shop, rating, rating count, stock, postage included, point rate, catch copy, caption, images |
| `shop rakuten item <shop:itemid>` | One item, same fields |
| `shop <iherb\|ikea\|nitori\|uniqlo\|cainz\|monotaro> search <query>` | Total count, page, items: id, name, URL, brand, price, list price, rating, rating count, stock, badges, variants, image |
| `shop <iherb\|ikea\|nitori\|uniqlo\|cainz\|monotaro> item <id\|url>` | Same fields plus categories, specification rows (`details`) and description. Cainz adds delivery and pickup stock rows; Monotaro adds order code, part number and ship lead time, or one row per variant for a product group |
| `shop <hotpepper\|tabelog\|tablecheck> search [query] [-a area]` | Total count, page, places: id, name, URL, genre, area, access, rating, rating count, budget, badges, image |
| `shop <hotpepper\|tabelog\|tablecheck> item <id\|url>` | Same fields plus address, hours, closed days, phone, booking URL, detail rows, menus (coupons, courses) and description |
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
| `--range N` | amazon price | Days of history, default 365 |
| `--width`, `--height` | amazon price | Chart size in pixels, default 1200x350 |
| `-o`, `--output PATH` | amazon price | PNG path, default `<asin>-<range>d.png` in the current directory |
| `-n N` | search | Amazon, iHerb, IKEA, Nitori, Uniqlo, Cainz, Monotaro: keep the first N results. Rakuten: results per page, max 30 |
| `--sort` | rakuten search | `standard`, `price-asc`, `price-desc`, `reviews`, `rating`, `newest` |
| `--min-price`, `--max-price` | rakuten search | Price range in yen |
| `--sort` | iherb, ikea, nitori, uniqlo search | `relevance`, `price-asc`, `price-desc`, `reviews`, `newest` |
| `--sort` | cainz search | `relevance`, `price-asc`, `price-desc` |
| `--sort` | monotaro search | `relevance`, `price-asc`, `price-desc`, `reviews` (review count) |
| `-a`, `--area` | hotpepper, tabelog, tablecheck search | Area, station or prefecture, e.g. `渋谷` |
| `--sort` | hotpepper, tabelog, tablecheck search | `relevance`, `rating`, `reviews`, `newest`; each site supports a subset |
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
| 4 | Page not found, or a refusal disguised as one (`amazon price`: Keepa has no data for the ASIN) |

## Rakuten API

Endpoint `https://openapi.rakuten.co.jp/ichibams/api/IchibaItem/Search/20260701`, `formatVersion=2`. Limits: about one request per second per app, 30 items per page, 3,000 results per query.

## Keepa chart

`amazon price` downloads `https://graph.keepa.com/pricehistory.png?asin=<ASIN>&domain=<co.jp|com>&range=<days>&amazon=1&new=1&width=<w>&height=<h>`, which needs no key. A non-200 status or a body that is not a PNG is an error; HTTP 403, 429 and 503 exit with code 3.

## Shop endpoints

| Connector | Search | Item | IDs accepted |
|---|---|---|---|
| `iherb` | `jp.iherb.com/search` HTML, 48 per page | `catalog.app.iherb.com/product/<id>` JSON | `70316`, `jp.iherb.com/pr/<slug>/<id>` |
| `ikea` | `sik.search.blue.cdtapps.com/jp/ja/search-result-page` JSON, up to 500 results | `ikea.com/jp/ja/p/-<id>/` JSON-LD and HTML, stock from `api.salesitem.ingka.com` | `40432112`, `404.321.12`, `s49434096`, product URL |
| `nitori` | `nitori-net.jp/occ/v2/nitorinet/nitori/products/search` JSON | `.../products/<code>` JSON | `8761161s`, `nitori-net.jp/ec/product/<code>/` |
| `cainz` | `cainz.com/search/?q=` page, Next.js server payload, 40 per page (80 with `-n` over 40) | `cainz.com/g/<jan>.html` server payload and JSON-LD, stock from the page | `4549509677048`, product URL |
| `monotaro` | `monotaro.com/s/?q=` page, Next.js server payload, 40 product groups per page; no match is HTTP 404 | `monotaro.com/g/<group>/` or `/p/<4>/<4>/` server payload and JSON-LD | group `00005028`, order code `02196704`, group or item URL |
| `uniqlo` | `uniqlo.com/jp/api/commerce/v5/ja/products` JSON, up to 100 per page | `.../products/<id>/price-groups/<pg>/details` and `/l2s` stock JSON | `E465185-000`, `465185`, product URL |

Prices are integers in yen. `price` is what the shop charges now; `list_price` is set only when the shop shows a higher regular price. Cainz and Monotaro prices include tax.

## Place endpoints

| Connector | Search | Item | IDs accepted |
|---|---|---|---|
| `hotpepper` | `beauty.hotpepper.jp/CSP/bt/salonSearch/search/` HTML, 20 per page; `--area` joins the free word; only `relevance` sort | `beauty.hotpepper.jp/slnH<id>/` HTML and JSON-LD, coupons and menus from `/coupon/` | `H000419288`, salon or booking URL |
| `tabelog` | `tabelog.com/rst/rstsearch` HTML; `--area` resolved through the site's suggest API; sorts `rating`, `reviews`, `newest` (new openings) | Restaurant page HTML and JSON-LD, courses from the top page | `13003367`, `tokyo/A1303/A130301/13003367`, restaurant URL |
| `tablecheck` | TableCheck search API (the key the web app ships), 20 per call; a cuisine word searches by cuisine, other text by name; only `relevance` sort | `tablecheck.com/ja/<slug>` embedded app state, courses included | `sushi-ojima`, shop or reserve URL |

Ratings: Tabelog scores out of 5 with two decimals, HotPepper Beauty review averages (search pages only carry the review count), TableCheck has none. `budget` is the price text the site shows (cut price, lunch and dinner ranges). Menu `price` is in yen, `price_text` keeps the site's wording.

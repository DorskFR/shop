# shop

Searches Japanese shops with pure HTTP calls, fast and lightweight.

[![release](https://img.shields.io/github/v/release/DorskFR/shop)](https://github.com/DorskFR/shop/releases/latest)
[![CI](https://github.com/DorskFR/shop/actions/workflows/ci.yml/badge.svg)](https://github.com/DorskFR/shop/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/DorskFR/shop?style=flat)](LICENSE)

![shop searching Amazon for electric kettles, showing one product's details, then searching Rakuten sorted by reviews](docs/assets/demo.gif)

Results come back as text or JSON: prices, ratings, specs and top reviews. No browser, no API key except Rakuten's free one.

| Connector | Source |
|---|---|
| `amazon` | Public amazon.co.jp pages, Keepa's public price chart |
| `rakuten` | Rakuten Ichiba Item Search API |
| `iherb` | jp.iherb.com search pages and the iHerb catalog API |
| `ikea` | IKEA Japan search service, product pages and stock API |
| `nitori` | nitori-net.jp storefront API |
| `uniqlo` | uniqlo.com/jp commerce API |
| `cainz` | cainz.com search and product pages |
| `monotaro` | monotaro.com search, product group and item pages |
| `hotpepper` | HotPepper Beauty salon pages (hair salons, barbers) |
| `tabelog` | Tabelog restaurant pages |
| `tablecheck` | TableCheck restaurant booking pages |

```sh
shop amazon search 電気ケトル --no-ads     # results with price, list price, rating, badges
shop amazon item B01BET65BI               # price, seller, stock, rating histogram, specs
shop amazon reviews B01BET65BI            # top reviews from the product page
shop amazon price B01BET65BI --range 90   # Keepa price history chart as a PNG
shop rakuten search 電気ケトル --sort reviews --max-price 5000
shop --json amazon search 電気ケトル       # same data as JSON
shop ikea search ソファ --sort price-asc   # iherb, ikea, nitori, uniqlo, cainz, monotaro share search and item
shop monotaro search 六角レンチ            # tax-included price, part number, ship lead time
shop cainz item 4549509677048             # specs, delivery and store pickup stock
shop uniqlo item E465185-000              # price, rating, stock, colors and sizes, specs
shop tabelog search ラーメン -a 渋谷 --sort rating   # hotpepper, tabelog, tablecheck: places
shop hotpepper item H000419288            # rating, access, hours, coupons and menus
```

## Install

Prebuilt binaries for Linux (amd64, arm64) and macOS (arm64) are on the [releases page](https://github.com/DorskFR/shop/releases/latest).

```sh
cargo install --git https://github.com/DorskFR/shop
```

Shell completion:

```sh
shop completions bash > ~/.local/share/bash-completion/completions/shop
shop completions zsh > "${fpath[1]}/_shop"
shop completions fish > ~/.config/fish/completions/shop.fish
```

`shop update` installs the latest release after checking its SHA-256. Interactive runs print a notice once a day when a newer release exists; `SHOP_NO_UPDATE_CHECK=1` turns it off.

## Quick start

```console
$ shop amazon search 電気ケトル -n 2 --no-ads
 1. ￥1,529  ★4.1 (17268)  list ￥1,670
    ヒロコーポレーション ヒロ・コーポレーション 中身が見やすい 電気ケトル 保証付き 空焚き防止機能付き 1.0L コンパクト グレー KTK-300-G
    過去1か月で6000点以上購入されました
    https://www.amazon.co.jp/dp/B01BET65BI
 2. ￥3,852  ★4.2 (336)  [ベストセラー]
    ティファール 電気ケトル 「ジャスティン ロック」1.2L ホワイト KO5901JPA
    過去1か月で1000点以上購入されました
    https://www.amazon.co.jp/dp/B0F93QQJQJ
```

## Usage

### Inspect a product

```console
$ shop amazon item https://www.amazon.co.jp/dp/B01BET65BI
ヒロコーポレーション ヒロ・コーポレーション 中身が見やすい 電気ケトル 保証付き 空焚き防止機能付き 1.0L コンパクト グレー KTK-300-G
https://www.amazon.co.jp/dp/B01BET65BI
brand        ブランド: Hiro Corporation
price        ￥1,529
list price   ￥1,670
availability 通常2～3日以内に発送します。
seller       アイリスオーヤマ公式ショップ
rating       ★4.1 (17268)
             5★ 53%  4★ 24%  3★ 13%  2★ 4%  1★ 6%
```

### Pipe results into other tools

```console
$ shop --json amazon search 電気ケトル --no-ads -n 4 | jq -r '.[] | [.price_value, .rating, .rating_count, .asin] | @tsv'
1529.0	4.1	17268	B01BET65BI
3852.0	4.2	336	B0F93QQJQJ
1440.0	4.1	17268	B081NSM9V9
3852.0	4.4	5856	B0CG18ZM6J
```

All flags are in `shop --help`. Fields and exit codes are in [docs/reference.md](docs/reference.md).

## Configuration

Rakuten needs a free app from [Rakuten Web Service](https://webservice.rakuten.co.jp/app/create):

| Variable | Effect |
|---|---|
| `RAKUTEN_APP_ID` | Application ID, required |
| `RAKUTEN_ACCESS_KEY` | Access key, required, sent as a header |
| `RAKUTEN_REFERER` | Sent as `Referer` and `Origin` when the app restricts allowed websites |

## Limitations

- Amazon shows full review lists only to signed-in users, so `reviews` returns the 5 to 13 top reviews from the product page.
- amazon.com refuses plain HTTP from Japanese IPs. `-m com` exits with code 3 there.
- iHerb search sometimes gets a Cloudflare challenge; `shop` retries, then exits with code 3. iHerb items have no variants.
- Nitori answers in 8 to 10 seconds per request and reports no list price or item stock.
- IKEA search has no offset, so page N fetches N pages of results. Stock is country-wide, not per store.
- Cainz store stock is for the default pickup store the site picks; choosing a store needs a login. Cainz has no `reviews` or `newest` sort.
- Monotaro search lists product groups; `item` on a group shows up to 50 variants with their order codes and part numbers. Monotaro has no `newest` sort.
- DCM (dcm-ekurashi.com) is not supported: DCM Online is suspended for a security inspection and every page redirects to a maintenance notice.
- Muji is not supported: muji.com does not answer plain HTTP clients.
- `price` saves Keepa's chart image only. Numeric price history needs a paid Keepa API key.
- Scraping is against Amazon's conditions of use. Heavy use gets the IP challenged (exit code 3).
- Rakuten returns review averages and counts, not review text.

## Development

```sh
git clone https://github.com/DorskFR/shop && cd shop
cargo test
```

Fixtures and the demo are described in [docs/development.md](docs/development.md).

## License

[MIT](LICENSE)

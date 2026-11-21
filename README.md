# shop

Searches Amazon Japan and Rakuten Ichiba from the terminal: prices, ratings, specs and top reviews as text or JSON.

[![release](https://img.shields.io/github/v/release/DorskFR/shop)](https://github.com/DorskFR/shop/releases/latest)
[![CI](https://github.com/DorskFR/shop/actions/workflows/ci.yml/badge.svg)](https://github.com/DorskFR/shop/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/DorskFR/shop)](LICENSE)

![shop searching Amazon for electric kettles, showing one product's details, then searching Rakuten sorted by reviews](docs/assets/demo.gif)

Amazon has no free product API, so `shop` parses the public amazon.co.jp pages over plain HTTPS. Rakuten goes through its official Ichiba Item Search API.

```sh
shop amazon search 電気ケトル --no-ads     # results with price, list price, rating, badges
shop amazon item B01BET65BI               # price, seller, stock, rating histogram, specs
shop amazon reviews B01BET65BI            # top reviews from the product page
shop rakuten search 電気ケトル --sort reviews --max-price 5000
shop --json amazon search 電気ケトル       # same data as JSON
```

## Install

Prebuilt binaries for Linux (amd64, arm64) and macOS (arm64) are on the [releases page](https://github.com/DorskFR/shop/releases/latest).

```sh
cargo install --git https://github.com/DorskFR/shop
```

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

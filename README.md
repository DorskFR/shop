# shop

Search and inspect products on Amazon (amazon.co.jp, amazon.com) and Rakuten Ichiba from the command line. Text output for people, `--json` for agents and scripts.

```console
$ shop amazon search 骨伝導 イヤホン -n 2 --no-ads
 1. ￥2,299  ★4.9 (64)  list ￥10,999  [限定プライムセール]
    骨伝導イヤホン ワイヤレスイヤホン Bluetooth 5.4 28g超軽量 16mmドライバー 最大10時間再生 ENC通話 オープンイヤー
    過去1か月で100点以上購入されました
    https://www.amazon.co.jp/dp/B0HFQTT2ND
 2. ￥2,399  ★4.8 (67)  list ￥11,299  [Amazon おすすめ]  [タイムセール]
    EQZ 骨伝導イヤホン こつでんどう イヤホン bluetooth V19 骨伝導 ワイヤレス オープンイヤー ...
    https://www.amazon.co.jp/dp/B0HJVSRHNG

$ shop amazon item B0CT857V89
$ shop amazon reviews https://www.amazon.co.jp/dp/B0CT857V89
$ shop --json amazon -m com search usb c cable
$ shop rakuten search イヤホン --sort reviews --max-price 5000 --in-stock
$ shop rakuten item shop-code:item-id
```

## Amazon

No official API is usable (PA-API 5 was shut down in May 2026; the Creators API requires an Associates account with recent sales), so `shop` reads the public pages.

| Command | Data |
|---|---|
| `amazon search <query>` | ASIN, title, price, list price, rating, rating count, sponsored flag, badges, "bought in past month", delivery, image. `--page`, `--sort relevance\|price-asc\|price-desc\|reviews\|newest`, `--no-ads`, `-n` |
| `amazon item <asin\|url>` | Title, brand, price, availability, seller, rating histogram, categories, bullet points, specification table, image, top reviews |
| `amazon reviews <asin\|url>` | The top reviews embedded in the product page (usually 5–13) |

`-m jp` (default) or `-m com` picks the marketplace; a product URL picks it automatically.

**Fetching.** `--fetch auto` (default) uses plain HTTPS and falls back to headless Chrome when Amazon answers with a captcha, a 503 or a challenge page. `--fetch http` and `--fetch browser` force one path. Chrome is found via `SHOP_CHROME`, `PATH`, macOS `/Applications` or the Playwright cache (`~/.cache/ms-playwright`). If Chrome has no usable sandbox (Ubuntu's AppArmor user-namespace restriction with Playwright's Chromium), it is retried with `--no-sandbox`; `SHOP_CHROME_NO_SANDBOX=1` skips the first attempt.

**Limits.**
- Full review lists need a logged-in session (Amazon put them behind sign-in in late 2024); only the top reviews are available.
- Observed from a Japanese residential IP: amazon.co.jp works over plain HTTP; amazon.com search needs the Chrome fallback; amazon.com product pages are refused (fake 404) even to Chrome. Prices on amazon.com follow the visitor's location.
- Keep the volume low and personal. Scraping is against Amazon's conditions of use, and heavy use gets the IP challenged.

## Rakuten

Uses the official [Ichiba Item Search API](https://webservice.rakuten.co.jp/documentation/ichiba-item-search) (version 2026-07-01). Register a free app at <https://webservice.rakuten.co.jp/app/create> and set:

| Variable | |
|---|---|
| `RAKUTEN_APP_ID` | Application ID |
| `RAKUTEN_ACCESS_KEY` | Access key, sent as a header |
| `RAKUTEN_REFERER` | Optional. Sent as `Referer`/`Origin` when the app restricts allowed websites |

The API allows about one request per second, 30 items per page and 3,000 results per query. It returns review averages and counts, not review text.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | OK |
| 1 | Error (bad input, API error, layout change) |
| 3 | Blocked by Amazon (captcha / challenge) |
| 4 | Page not found (or disguised refusal) |

## Install

Binaries for linux amd64/arm64 and macOS arm64 are attached to each [release](https://github.com/DorskFR/shop/releases/latest), or:

```sh
cargo install --git https://github.com/DorskFR/shop
```

## Development

```sh
cargo test     # parsers run against saved pages in tests/fixtures
```

When Amazon changes its layout, save the failing page into `tests/fixtures/` (scripts and styles stripped), fix the selectors in `src/amazon.rs`, and extend the assertions in `tests/fixtures.rs`.

# Development

## Tests

```sh
cargo test
```

`tests/fixtures/<shop>/` holds trimmed iHerb, IKEA, Nitori, Uniqlo, Cainz, Monotaro, Apple, HotPepper Beauty, Tabelog and TableCheck responses tested by `tests/<shop>.rs`. `tests/fixtures/` holds amazon.co.jp and amazon.com pages saved with `<script>` and `<style>` removed, plus the captcha, 503 and not-found pages Amazon serves to clients it refuses. `tests/fixtures/keepa/chart.png` is a 300x150 Keepa chart for `tests/keepa.rs`. `tests/fixtures.rs` runs the parsers against them.

When Amazon changes its layout:

1. Save the page: `curl -s --compressed -A '<browser user agent>' -H 'Accept-Language: ja-JP' -o tests/fixtures/<name>.html '<url>'`.
2. Strip `<script>` and `<style>` blocks.
3. Fix the selectors in `src/amazon.rs` and extend the assertions in `tests/fixtures.rs`.

## Endpoint overrides

| Variable | Effect |
|---|---|
| `SHOP_AMAZON_ORIGIN` | Sends Amazon requests to this origin instead of `https://www.amazon.co.jp` (printed URLs are unchanged) |
| `SHOP_RAKUTEN_ENDPOINT` | Replaces the Rakuten Item Search endpoint |

## Demo GIF

`docs/assets/demo.gif` is rendered by [VHS](https://github.com/charmbracelet/vhs) from `docs/demo/demo.tape`. The tape starts `python3 -m http.server` on `docs/demo/mock/`, which holds invented products, and points `shop` at it through the overrides above.

```sh
docs/demo/render.sh
```

The script builds the release binary and runs the VHS container with the host's Noto CJK fonts mounted for Japanese glyphs.

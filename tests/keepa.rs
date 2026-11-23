use shop::{
    amazon::{Market, parse_asin},
    fetch::FetchError,
    keepa::{self, ChartOptions},
};

const OPTS: ChartOptions = ChartOptions {
    range: 365,
    width: 1200,
    height: 350,
};

fn chart() -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/keepa/chart.png",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn asin_from_input() {
    assert_eq!(parse_asin("B09R9T8DV7"), Some(("B09R9T8DV7".into(), None)));
    assert_eq!(
        parse_asin("https://www.amazon.co.jp/dp/B09R9T8DV7?th=1"),
        Some(("B09R9T8DV7".into(), Some(Market::Jp)))
    );
    assert_eq!(
        parse_asin("https://amazon.com/Some-Thing/dp/b09r9t8dv7/ref=x"),
        Some(("B09R9T8DV7".into(), Some(Market::Com)))
    );
}

#[test]
fn chart_urls() {
    assert_eq!(
        keepa::chart_url(Market::Jp, "B09R9T8DV7", OPTS),
        "https://graph.keepa.com/pricehistory.png?asin=B09R9T8DV7&domain=co.jp&range=365&amazon=1&new=1&width=1200&height=350"
    );
    let com = ChartOptions {
        range: 90,
        width: 600,
        height: 200,
    };
    assert_eq!(
        keepa::chart_url(Market::Com, "B07V78HMDQ", com),
        "https://graph.keepa.com/pricehistory.png?asin=B07V78HMDQ&domain=com&range=90&amazon=1&new=1&width=600&height=200"
    );
}

#[test]
fn keepa_product_urls() {
    assert_eq!(
        keepa::product_url(Market::Jp, "B09R9T8DV7"),
        "https://keepa.com/#!product/5-B09R9T8DV7"
    );
    assert_eq!(
        keepa::product_url(Market::Com, "B09R9T8DV7"),
        "https://keepa.com/#!product/1-B09R9T8DV7"
    );
    assert_eq!(
        keepa::default_path("B09R9T8DV7", 365),
        "B09R9T8DV7-365d.png"
    );
}

#[test]
fn accepts_png() {
    let png = chart();
    assert!(keepa::is_png(&png));
    keepa::check("B09R9T8DV7", 200, Some("image/png"), Some("OK"), &png).unwrap();
    keepa::check("B09R9T8DV7", 200, None, None, &png).unwrap();
}

#[test]
fn rejects_non_png() {
    let err = keepa::check(
        "B09R9T8DV7",
        200,
        Some("text/html"),
        None,
        b"<html>Too many requests</html>",
    )
    .unwrap_err();
    assert!(err.to_string().contains("text/html instead of a PNG"));
}

#[test]
fn rejects_bad_status() {
    let png = chart();
    let err = keepa::check("B09R9T8DV7", 429, Some("image/png"), None, &png).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<FetchError>(),
        Some(FetchError::Blocked(_))
    ));
    let err = keepa::check("B09R9T8DV7", 500, None, None, b"").unwrap_err();
    assert!(err.to_string().contains("HTTP 500"));
}

#[test]
fn unknown_asin() {
    let png = chart();
    let err = keepa::check("ZZZZZZZZZZ", 200, Some("image/png"), Some("FAIL"), &png).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
}

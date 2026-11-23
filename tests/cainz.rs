use shop::{
    cainz::{parse_id, parse_item, parse_search, search_url},
    product::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/cainz/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query(text: &str, page: u32, limit: Option<u32>, sort: Sort) -> Query<'_> {
    Query {
        text,
        page,
        limit,
        sort,
    }
}

#[test]
fn search_fixture() {
    let page = parse_search(&fixture("search.html")).unwrap();
    assert_eq!(page.total, Some(13312));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 3);
    let first = &page.items[0];
    assert_eq!(first.id, "4549509677048");
    assert_eq!(first.url, "https://www.cainz.com/g/4549509677048.html");
    assert_eq!(
        first.name,
        "Kumimoku 3.6V充電式ミニドリル＆ドライバー ブルー KDC-02A"
    );
    assert_eq!(first.brand.as_deref(), Some("MAKE"));
    assert_eq!(first.price, Some(2950));
    assert_eq!(first.rating, Some(4.6));
    assert_eq!(first.rating_count, Some(17));
    assert_eq!(first.badges, ["毎日安い", "セール"]);
    assert_eq!(first.variants, ["カラー・柄: カーキ", "カラー・柄: ブルー"]);
    assert!(
        first
            .image
            .as_deref()
            .unwrap()
            .ends_with("4549509677048_01.jpg")
    );

    let unrated = &page.items[1];
    assert_eq!(unrated.brand, None, "\"なし\" is not a brand");
    assert_eq!(unrated.rating, None);
    assert_eq!(unrated.rating_count, Some(0));
    assert_eq!(page.items[2].badges, ["配送のみ"]);
}

#[test]
fn search_page_number_as_string() {
    let page = parse_search(&fixture("search_page2.html")).unwrap();
    assert_eq!(page.page, 2);
    assert_eq!(page.items.len(), 2);
}

#[test]
fn search_empty_and_broken() {
    let page = parse_search(&fixture("search_empty.html")).unwrap();
    assert_eq!(page.total, Some(0));
    assert!(page.items.is_empty());
    assert!(parse_search("<html><body>maintenance</body></html>").is_err());
}

#[test]
fn item_fixture() {
    let p = parse_item(&fixture("item.html")).unwrap();
    assert_eq!(p.id, "4549509677048");
    assert_eq!(p.url, "https://www.cainz.com/g/4549509677048.html");
    assert_eq!(p.brand.as_deref(), Some("MAKE"));
    assert_eq!(p.price, Some(2950));
    assert_eq!(p.in_stock, Some(true));
    assert_eq!(p.rating, Some(4.6));
    assert_eq!(p.rating_count, Some(17));
    assert_eq!(
        p.badges,
        [
            "セール",
            "配送で取寄せ可",
            "店舗受取で取寄せ可",
            "カインズオリジナル"
        ]
    );
    assert_eq!(
        p.categories.first().map(String::as_str),
        Some("工具・建築資材・作業着・木材・DIY")
    );
    assert_eq!(
        p.categories.last().map(String::as_str),
        Some("充電式ドリルドライバー")
    );
    let detail = |k: &str| {
        p.details
            .iter()
            .find(|(dk, _)| dk == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(detail("カラー"), Some("ブルー"));
    assert_eq!(detail("重量"), Some("0.34kg"));
    assert_eq!(detail("在庫 店舗受取"), Some("在庫 9点 (店舗コード 814)"));
    assert_eq!(detail("在庫 配送"), Some("在庫 84点"));
    assert_eq!(p.details[0].0, "カラー", "specs keep the site's sort order");
    assert!(
        p.description
            .as_deref()
            .unwrap()
            .starts_with("本体重量約340g")
    );
}

#[test]
fn item_without_product_data_is_not_found() {
    let err = parse_item("<html><title>ホームセンター通販のカインズ</title></html>").unwrap_err();
    assert!(matches!(
        err.downcast_ref::<shop::fetch::FetchError>(),
        Some(shop::fetch::FetchError::NotFound(_))
    ));
}

#[test]
fn ids() {
    assert_eq!(parse_id("4549509677048").as_deref(), Some("4549509677048"));
    assert_eq!(
        parse_id("https://www.cainz.com/g/4549509677048.html?x=1").as_deref(),
        Some("4549509677048")
    );
    assert_eq!(parse_id("abc"), None);
    assert_eq!(parse_id("https://www.cainz.com/g/.html"), None);
}

#[test]
fn urls() {
    assert_eq!(
        search_url(&query("ドライバー", 1, None, Sort::Relevance)).unwrap(),
        "https://www.cainz.com/search/?q=%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC"
    );
    assert_eq!(
        search_url(&query("x", 3, Some(60), Sort::PriceAsc)).unwrap(),
        "https://www.cainz.com/search/?q=x&sortRule=lowPrice&displaySize=80&page=3"
    );
    assert!(
        search_url(&query("x", 1, None, Sort::PriceDesc))
            .unwrap()
            .contains("sortRule=highPrice")
    );
    assert!(search_url(&query("x", 1, None, Sort::Newest)).is_err());
    assert!(search_url(&query("x", 1, None, Sort::Reviews)).is_err());
}

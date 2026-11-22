use shop::{
    fetch::FetchError,
    ikea::{self, check_status, parse_availability, parse_id, parse_item, parse_search},
    product::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/ikea/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query(sort: Sort, page: u32, limit: Option<u32>) -> Query<'static> {
    Query {
        text: "ソファ",
        page,
        limit,
        sort,
    }
}

#[test]
fn search_items() {
    let r = parse_search(&fixture("search_sofa.json")).unwrap();
    assert_eq!(r.total, Some(282));
    assert!(r.items.len() >= 4);
    let first = &r.items[0];
    assert_eq!(first.id, "s99484999");
    assert_eq!(
        first.name,
        "SÖDERHAMN ソーデルハムン 3人掛けコンパクトソファセクション, フリードトゥナ ライトベージュ"
    );
    assert_eq!(first.price, Some(42990));
    assert_eq!(first.list_price, None);
    assert_eq!(first.rating, Some(4.4));
    assert_eq!(first.rating_count, Some(58));
    assert_eq!(first.brand.as_deref(), Some("IKEA"));
    assert_eq!(
        first.url,
        "https://www.ikea.com/jp/ja/p/soederhamn-compact-3-seat-section-fridtuna-light-beige-s99484999/"
    );
    assert!(first.badges.contains(&"人気商品".to_string()));
    assert_eq!(first.categories[0], "ソファ＆パーソナルチェア");
    assert!(
        first
            .image
            .as_deref()
            .unwrap()
            .ends_with("__1577796_pe1033541_s5.jpg")
    );
    assert_eq!(first.variants.len(), 2);
    assert_eq!(r.items[2].id, "10489009");
    assert_eq!(r.items[2].price, Some(11990));
}

#[test]
fn search_discount_and_new() {
    let r = parse_search(&fixture("search_sofa.json")).unwrap();
    let sale = r.items.iter().find(|p| p.id == "s19471463").unwrap();
    assert_eq!(sale.price, Some(61990));
    assert_eq!(sale.list_price, Some(66990));
    assert!(sale.badges.contains(&"IKEA Family価格".to_string()));
    assert!(
        r.items
            .iter()
            .any(|p| p.badges.contains(&"New".to_string()))
    );
}

#[test]
fn search_empty_and_garbage() {
    let r = parse_search(&fixture("search_empty.json")).unwrap();
    assert_eq!(r.total, Some(0));
    assert!(r.items.is_empty());
    assert!(parse_search("<html>nope</html>").is_err());
    assert!(parse_search("{}").is_err());
}

#[test]
fn search_url_and_sort() {
    let u = ikea::search_url(&query(Sort::Relevance, 1, None)).unwrap();
    assert!(u.starts_with("https://sik.search.blue.cdtapps.com/jp/ja/search-result-page?"));
    assert!(u.contains("types=PRODUCT"));
    assert!(u.contains("size=24"));
    assert!(!u.contains("sort="));
    for (sort, want) in [
        (Sort::PriceAsc, "sort=PRICE_LOW_TO_HIGH"),
        (Sort::PriceDesc, "sort=PRICE_HIGH_TO_LOW"),
        (Sort::Reviews, "sort=RATING"),
        (Sort::Newest, "sort=NEWEST"),
    ] {
        assert!(
            ikea::search_url(&query(sort, 1, None))
                .unwrap()
                .contains(want)
        );
    }
}

#[test]
fn paging_window() {
    assert_eq!(
        ikea::page_window(&query(Sort::Relevance, 1, None)),
        (24, 0, 24)
    );
    assert_eq!(
        ikea::page_window(&query(Sort::Relevance, 3, Some(10))),
        (30, 20, 10)
    );
    assert_eq!(
        ikea::page_window(&query(Sort::Relevance, 0, Some(5))),
        (5, 0, 5)
    );
    let u = ikea::search_url(&query(Sort::Relevance, 3, Some(10))).unwrap();
    assert!(u.contains("size=30"));
    assert!(u.contains("q=%E3%82%BD%E3%83%95%E3%82%A1"));
}

#[test]
fn id_parsing() {
    for ok in [
        "40432112",
        "404.321.12",
        " 404.321.12 ",
        "https://www.ikea.com/jp/ja/p/glostad-2-seat-sofa-knisa-dark-grey-40432112/",
        "www.ikea.com/jp/ja/p/glostad-2-seat-sofa-knisa-dark-grey-40432112/?x=1#content",
    ] {
        assert_eq!(parse_id(ok).as_deref(), Some("40432112"), "{ok}");
    }
    assert_eq!(parse_id("s49434096").as_deref(), Some("s49434096"));
    assert_eq!(parse_id("S494.340.96").as_deref(), Some("s49434096"));
    assert_eq!(
        parse_id("https://www.ikea.com/jp/ja/p/vimle-3-seat-sofa-bed-s59639192/").as_deref(),
        Some("s59639192")
    );
    for bad in [
        "",
        "abc",
        "1234567",
        "123456789",
        "https://example.com/jp/ja/p/foo-40432112/",
        "https://www.ikea.com/jp/ja/cat/sofas-fu003/",
    ] {
        assert_eq!(parse_id(bad), None, "{bad}");
    }
    assert_eq!(
        ikea::item_url("s49434096"),
        "https://www.ikea.com/jp/ja/p/-s49434096/"
    );
}

#[test]
fn item_regular() {
    let p = parse_item(&fixture("item_glostad.html"), "10489009").unwrap();
    assert_eq!(p.id, "10489009");
    assert_eq!(
        p.name,
        "GLOSTAD グロスタード 2人掛けソファ, クニーサ ダークグレー"
    );
    assert_eq!(
        p.url,
        "https://www.ikea.com/jp/ja/p/glostad-2-seat-sofa-knisa-dark-grey-10489009/"
    );
    assert_eq!(p.brand.as_deref(), Some("IKEA"));
    assert_eq!(p.price, Some(11990));
    assert_eq!(p.list_price, None);
    assert_eq!(p.rating, Some(4.5));
    assert_eq!(p.rating_count, Some(642));
    assert_eq!(p.in_stock, Some(true));
    assert!(p.badges.is_empty());
    assert_eq!(
        p.categories,
        [
            "ソファ＆パーソナルチェア",
            "ソファ",
            "2人掛けソファ・コンパクトソファ"
        ]
    );
    assert!(
        p.variants
            .contains(&"カバー: クニーサ ダークグレー".to_string())
    );
    assert!(
        p.variants
            .contains(&"カバー: クニーサ ミディアムブルー".to_string())
    );
    let d = |k: &str| {
        p.details
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(d("商品番号"), Some("104.890.09"));
    assert_eq!(d("デザイナー"), Some("Jonas Hultqvist"));
    assert_eq!(d("幅"), Some("121 cm"));
    assert_eq!(d("奥行き"), Some("78 cm"));
    assert_eq!(d("パッケージ 重さ"), Some("20.15 kg"));
    assert_eq!(
        d("シート"),
        Some("ポリウレタンフォーム 25kg/立方メートル, フェルトライナー")
    );
    assert_eq!(p.details.iter().filter(|(k, _)| k == "シート").count(), 1);
    assert!(
        p.description
            .as_deref()
            .unwrap()
            .starts_with("GLOSTAD/グロスタード ソファなら")
    );
    assert!(!p.description.as_deref().unwrap().contains('<'));
    assert!(
        p.image
            .as_deref()
            .unwrap()
            .contains("glostad-2-seat-sofa-knisa-dark-grey")
    );
}

#[test]
fn item_family_price() {
    let p = parse_item(&fixture("item_family.html"), "s19471463").unwrap();
    assert_eq!(p.id, "s19471463");
    assert_eq!(p.price, Some(61990));
    assert_eq!(p.list_price, Some(66990));
    assert_eq!(p.rating, Some(3.8));
    assert_eq!(p.rating_count, Some(73));
    assert!(p.badges.contains(&"IKEA Family価格".to_string()));
    assert!(p.badges.iter().any(|b| b.contains("5,000")));
    assert_eq!(p.variants.len(), 9);
    assert!(p.name.starts_with("JÄTTEBO イェッテボ 1.5人掛けモジュール"));
}

#[test]
fn item_limited_edition() {
    let p = parse_item(&fixture("item_limited.html"), "80628752").unwrap();
    assert_eq!(p.price, Some(14990));
    assert_eq!(
        p.name,
        "KOMPISHÄNG コンピスヘング デスク, グリーン パイン材, 100x55 cm"
    );
    assert_eq!(p.badges, ["限定コレクション"]);
    assert_eq!(p.rating, Some(4.8));
    assert_eq!(p.rating_count, Some(24));
}

#[test]
fn item_not_found() {
    let err = parse_item(&fixture("item_gone.html"), "40432112").unwrap_err();
    assert!(matches!(
        err.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
    assert!(parse_item("", "40432112").is_err());
}

#[test]
fn availability() {
    let m = parse_availability(&fixture("availability.json"));
    assert_eq!(m.get("10489009"), Some(&true));
    assert_eq!(m.get("80628752"), Some(&true));
    assert_eq!(m.get("00000000"), None);
    assert!(parse_availability("garbage").is_empty());
    let out = r#"{"availabilities":[{"availableForClickCollect":false,"availableForHomeDelivery":false,"itemKey":{"itemNo":"11111111"}}]}"#;
    assert_eq!(parse_availability(out).get("11111111"), Some(&false));
}

#[test]
fn status_handling() {
    assert!(check_status("search", 200, "{}").is_ok());
    for code in [403, 429] {
        let e = check_status("search", code, "denied").unwrap_err();
        assert!(matches!(
            e.downcast_ref::<FetchError>(),
            Some(FetchError::Blocked(_))
        ));
    }
    let e = check_status(
        "search",
        400,
        r#"{"status":400,"detail":"Invalid sort order: BOGUS"}"#,
    )
    .unwrap_err();
    assert!(e.to_string().contains("Invalid sort order: BOGUS"));
}

use shop::{
    fetch::FetchError,
    iherb::{html_to_text, is_challenge, parse_id, parse_item, parse_search, search_url},
    product::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/iherb/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query(text: &str, page: u32, sort: Sort) -> Query<'_> {
    Query {
        text,
        page,
        limit: None,
        sort,
    }
}

#[test]
fn search_page() {
    let page = parse_search(&fixture("search.html"), 1);
    assert_eq!(page.total, Some(16589));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 5);

    let d3 = &page.items[0];
    assert_eq!(d3.id, "70316");
    assert_eq!(
        d3.url,
        "https://jp.iherb.com/pr/california-gold-nutrition-vitamin-d3-125-mcg-5-000-iu-90-fish-gelatin-softgels/70316"
    );
    assert_eq!(d3.brand.as_deref(), Some("California Gold Nutrition"));
    assert!(d3.name.contains("ビタミンD3、125mcg"));
    assert_eq!(d3.price, Some(891));
    assert_eq!(d3.list_price, None);
    assert_eq!(d3.rating, Some(4.8));
    assert_eq!(d3.rating_count, Some(325478));
    assert_eq!(d3.in_stock, Some(true));
    assert!(d3.badges.iter().any(|b| b == "厳選アイテム"));
    assert_eq!(
        d3.image.as_deref(),
        Some(
            "https://cloudinary.images-iherb.com/image/upload/f_auto,q_auto:eco/images/cgn/cgn01065/u/404.jpg"
        )
    );
}

#[test]
fn search_sale_and_out_of_stock() {
    let page = parse_search(&fixture("search.html"), 1);
    let sale = page.items.iter().find(|p| p.id == "147638").unwrap();
    assert_eq!(sale.price, Some(3217));
    assert_eq!(sale.list_price, Some(7607));
    assert!(sale.badges.iter().any(|b| b == "58％OFF"));
    assert!(sale.badges.iter().any(|b| b == "お得商品"));

    let oos = page.items.iter().find(|p| p.id == "158863").unwrap();
    assert_eq!(oos.in_stock, Some(false));
    assert_eq!(oos.price, Some(764));
    assert_eq!(oos.rating_count, Some(13));
}

#[test]
fn search_empty_and_challenge() {
    let page = parse_search("<html><body></body></html>", 3);
    assert!(page.items.is_empty());
    assert_eq!(page.total, None);
    assert_eq!(page.page, 3);
    assert!(is_challenge(&fixture("challenge.html")));
    assert!(!is_challenge(&fixture("search.html")));
}

#[test]
fn sort_mapping() {
    let u = search_url(&query("ビタミンD", 1, Sort::Relevance)).unwrap();
    assert_eq!(
        u,
        "https://jp.iherb.com/search?kw=%E3%83%93%E3%82%BF%E3%83%9F%E3%83%B3D&sr=0"
    );
    for (sort, sr) in [
        (Sort::PriceAsc, "sr=4"),
        (Sort::PriceDesc, "sr=3"),
        (Sort::Reviews, "sr=12"),
        (Sort::Newest, "sr=10"),
    ] {
        assert!(search_url(&query("zinc", 1, sort)).unwrap().ends_with(sr));
    }
    assert_eq!(
        search_url(&query("zinc", 3, Sort::PriceAsc)).unwrap(),
        "https://jp.iherb.com/search?kw=zinc&sr=4&p=3"
    );
}

#[test]
fn id_parsing() {
    assert_eq!(parse_id("70316").as_deref(), Some("70316"));
    assert_eq!(parse_id(" 7069 ").as_deref(), Some("7069"));
    assert_eq!(
        parse_id("https://jp.iherb.com/pr/california-gold-nutrition-vitamin-d3-125-mcg-5-000-iu-360-fish-gelatin-softgels/70316")
            .as_deref(),
        Some("70316")
    );
    assert_eq!(
        parse_id("https://www.iherb.com/pr/x/123?rcode=ABC").as_deref(),
        Some("123")
    );
    assert_eq!(parse_id("abc"), None);
    assert_eq!(parse_id(""), None);
    assert_eq!(parse_id("https://example.com/pr/x/123"), None);
    assert_eq!(parse_id("https://jp.iherb.com/c/vitamin-d"), None);
}

#[test]
fn item() {
    let p = parse_item(&fixture("item.json")).unwrap();
    assert_eq!(p.id, "70316");
    assert_eq!(
        p.name,
        "ビタミンD3、125mcg（5,000 IU）、魚ゼラチンソフトジェル90粒"
    );
    assert_eq!(
        p.url,
        "https://jp.iherb.com/pr/california-gold-nutrition-vitamin-d3-125-mcg-5-000-iu-90-fish-gelatin-softgels/70316"
    );
    assert_eq!(
        p.brand.as_deref(),
        Some("California Gold Nutrition（カリフォルニアゴールドニュートリション）")
    );
    assert_eq!(p.price, Some(891));
    assert_eq!(p.list_price, None);
    assert_eq!(p.rating, Some(4.8));
    assert_eq!(p.rating_count, Some(325478));
    assert_eq!(p.in_stock, Some(true));
    assert_eq!(
        p.badges,
        ["厳選アイテム", "iHerbプライベートブランド", "iTested"]
    );
    assert_eq!(p.categories, ["ビタミンD", "ビタミン類", "サプリメント"]);
    assert_eq!(
        p.image.as_deref(),
        Some(
            "https://cloudinary.images-iherb.com/image/upload/f_auto,q_auto:eco/images/cgn/cgn01065/l/404.jpg"
        )
    );
    let detail = |k: &str| {
        p.details
            .iter()
            .find(|(name, _)| name == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(detail("商品コード"), Some("CGN-01065"));
    assert_eq!(detail("内容量"), Some("90 個"));
    assert_eq!(detail("賞味期限"), Some("2029年2月1日"));
    assert_eq!(
        detail("ランキング"),
        Some("ビタミンD 2位, ビタミン類 6位, サプリメント 16位")
    );
    assert!(detail("成分表示").unwrap().contains("625％"));
    let d = p.description.unwrap();
    assert!(d.starts_with("・ビタミンD3（コレカルシフェロール）配合"));
    assert!(!d.contains('<'));
}

#[test]
fn item_on_sale() {
    let p = parse_item(&fixture("item_sale.json")).unwrap();
    assert_eq!(p.id, "147638");
    assert_eq!(p.price, Some(3217));
    assert_eq!(p.list_price, Some(7607));
    assert!(p.badges.iter().any(|b| b == "58％OFF"));
}

#[test]
fn item_errors() {
    let e = parse_item(&fixture("item_missing.json")).unwrap_err();
    assert!(e.to_string().contains("does not exist"));
    assert!(parse_item("<html>").is_err());
    assert!(parse_item("{}").is_err());
}

#[test]
fn blocked_error_text() {
    let e: anyhow::Error = FetchError::Blocked("iHerb: challenge".into()).into();
    assert!(matches!(
        e.downcast_ref::<FetchError>(),
        Some(FetchError::Blocked(_))
    ));
}

#[test]
fn html_text() {
    assert_eq!(
        html_to_text("<ul><li>a</li><li>b&nbsp;c</li></ul><p>x<br>y</p>"),
        "・a\n・b c\nx\ny"
    );
}

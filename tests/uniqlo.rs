use shop::{
    fetch::FetchError,
    product::{Query, Sort},
    uniqlo::{parse_id, parse_item, parse_search, search_url},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/uniqlo/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query(sort: Sort, page: u32, limit: Option<u32>) -> Query<'static> {
    Query {
        text: "シャツ",
        page,
        limit,
        sort,
    }
}

#[test]
fn search_items() {
    let r = parse_search(&fixture("search.json"), 3).unwrap();
    assert_eq!(r.total, Some(251));
    assert_eq!(r.page, 3);
    assert_eq!(r.items.len(), 3);
    let a = &r.items[0];
    assert_eq!(a.id, "E486600-000");
    assert_eq!(a.name, "フランネルシャツ/ヘザー");
    assert_eq!(a.price, Some(2990));
    assert_eq!(a.list_price, None);
    assert_eq!(a.rating, Some(4.6));
    assert_eq!(a.rating_count, Some(56));
    assert_eq!(
        a.url,
        "https://www.uniqlo.com/jp/ja/products/E486600-000/00"
    );
    assert!(
        a.image
            .as_deref()
            .unwrap()
            .starts_with("https://image.uniqlo.com/")
    );
    let c = &r.items[2];
    assert_eq!(c.id, "E473179-000");
    assert_eq!(c.price, Some(3990));
    assert_eq!(c.rating_count, Some(1557));
    assert!(
        c.variants
            .iter()
            .any(|v| v.contains("XS") && v.contains("4XL"))
    );
}

#[test]
fn search_empty() {
    let r = parse_search(&fixture("search_empty.json"), 1).unwrap();
    assert_eq!(r.total, Some(0));
    assert!(r.items.is_empty());
}

#[test]
fn search_url_params() {
    let u = url::Url::parse(&search_url(&query(Sort::PriceAsc, 3, Some(20))).unwrap()).unwrap();
    assert_eq!(u.path(), "/jp/api/commerce/v5/ja/products");
    let p: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    assert_eq!(p["q"], "シャツ");
    assert_eq!(p["limit"], "20");
    assert_eq!(p["offset"], "40");
    assert_eq!(p["sort"], "2");
}

#[test]
fn sort_mapping() {
    let sort = |s| {
        let u = url::Url::parse(&search_url(&query(s, 1, None)).unwrap()).unwrap();
        u.query_pairs()
            .find(|(k, _)| k == "sort")
            .unwrap()
            .1
            .into_owned()
    };
    assert_eq!(sort(Sort::Relevance), "1");
    assert_eq!(sort(Sort::PriceAsc), "2");
    assert_eq!(sort(Sort::PriceDesc), "3");
    assert_eq!(sort(Sort::Reviews), "4");
    assert_eq!(sort(Sort::Newest), "5");
}

#[test]
fn ids() {
    let ok = |s: &str| parse_id(s).unwrap();
    assert_eq!(ok("E465185-000"), ("E465185-000".into(), "00".into()));
    assert_eq!(ok("465185"), ("E465185-000".into(), "00".into()));
    assert_eq!(
        ok("https://www.uniqlo.com/jp/ja/products/E465185-000/01?colorDisplayCode=01"),
        ("E465185-000".into(), "01".into())
    );
    assert_eq!(
        ok("https://www.uniqlo.com/jp/ja/products/E465185-000"),
        ("E465185-000".into(), "00".into())
    );
    assert!(parse_id("hello world").is_err());
    assert!(parse_id("").is_err());
}

#[test]
fn item() {
    let p = parse_item(&fixture("details.json"), Some(&fixture("l2s.json")), "00").unwrap();
    assert_eq!(p.id, "E465185-000");
    assert_eq!(p.name, "エアリズムコットンオーバーサイズTシャツ/5分袖");
    assert_eq!(
        p.url,
        "https://www.uniqlo.com/jp/ja/products/E465185-000/00"
    );
    assert_eq!(p.brand.as_deref(), Some("UNIQLO"));
    assert_eq!(p.price, Some(1990));
    assert_eq!(p.list_price, None);
    assert_eq!(p.rating, Some(4.5));
    assert_eq!(p.rating_count, Some(15260));
    assert_eq!(p.in_stock, Some(true));
    assert!(p.badges.iter().any(|b| b == "男女兼用"));
    assert!(p.badges.iter().any(|b| b == "ベストセラー"));
    assert_eq!(
        p.categories,
        [
            "Men",
            "フリース・スウェット・Tシャツ",
            "Tシャツ・カットソー",
            "Uniqlo U"
        ]
    );
    assert_eq!(p.variants.len(), 4);
    assert_eq!(p.variants[0], "00 WHITE: XS/S/M/L/XL/XXL/3XL/4XL");
    assert!(p.variants[1].starts_with("11 PINK: XS (sold out)/S (sold out)"));
    assert!(p.variants[1].ends_with("XXL/3XL/4XL"));
    assert!(p.variants[2].matches("sold out").count() == 8);
    let det: std::collections::HashMap<_, _> = p.details.iter().cloned().collect();
    assert!(det["素材"].starts_with("53% 綿"));
    assert!(!det["デザイン"].contains('<'));
    assert_eq!(det["原産国"], "BD, CN, ID, IN, KH, VN");
    let d = p.description.unwrap();
    assert!(d.starts_with("見た目コットン"));
    assert!(d.contains("ドロップショルダー"));
    assert!(p.image.unwrap().ends_with("_465185_3x4.jpg"));
}

#[test]
fn item_without_stock_data() {
    let p = parse_item(&fixture("details.json"), None, "00").unwrap();
    assert_eq!(p.in_stock, None);
    assert!(p.variants.iter().any(|v| v.starts_with("colors: ")));
}

#[test]
fn item_sold_out_and_promo() {
    let mut l: serde_json::Value = serde_json::from_str(&fixture("l2s.json")).unwrap();
    for s in l["result"]["stocks"].as_object_mut().unwrap().values_mut() {
        s["statusCode"] = "STOCK_OUT".into();
    }
    let mut d: serde_json::Value = serde_json::from_str(&fixture("details.json")).unwrap();
    d["result"]["prices"]["promo"] = serde_json::json!({"value": 1490});
    let p = parse_item(&d.to_string(), Some(&l.to_string()), "00").unwrap();
    assert_eq!(p.in_stock, Some(false));
    assert_eq!(p.price, Some(1490));
    assert_eq!(p.list_price, Some(1990));
}

#[test]
fn not_found_and_blocked() {
    let e = parse_item(&fixture("notfound.json"), None, "00").unwrap_err();
    assert!(matches!(
        e.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
    let e = parse_item(&fixture("blocked.html"), None, "00").unwrap_err();
    assert!(matches!(
        e.downcast_ref::<FetchError>(),
        Some(FetchError::Blocked(_))
    ));
    let e = parse_search(&fixture("blocked.html"), 1).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<FetchError>(),
        Some(FetchError::Blocked(_))
    ));
}

#[test]
fn garbage_json_does_not_panic() {
    assert!(
        parse_search(r#"{"status":"ok","result":{"items":[1,null,{}]}}"#, 1)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(parse_item(r#"{"status":"ok","result":{}}"#, None, "00").is_err());
}

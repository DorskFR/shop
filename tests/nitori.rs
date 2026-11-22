use shop::{
    fetch::FetchError,
    nitori::{self, parse_id, parse_item, parse_search, search_url},
    product::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/nitori/{name}",
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
    let page = parse_search(&fixture("search.json")).unwrap();
    assert_eq!(page.total, Some(260));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 3);
    let first = &page.items[0];
    assert_eq!(first.id, "2115100135618s");
    assert_eq!(
        first.url,
        "https://www.nitori-net.jp/ec/product/2115100135618s/"
    );
    assert!(first.name.starts_with("吸湿発熱 2枚合わせ毛布"));
    assert_eq!(first.brand.as_deref(), Some("ニトリ"));
    assert_eq!(first.price, Some(2990));
    assert_eq!(first.rating, Some(4.62));
    assert_eq!(first.rating_count, Some(39));
    assert_eq!(first.badges, ["一部値下げ", "2026年モデル"]);
    assert!(
        first
            .image
            .as_deref()
            .unwrap()
            .ends_with("2115100135618s00.jpg?ts=20261102091211675")
    );
    assert_eq!(page.items[1].price, Some(4990));
    assert_eq!(page.items[2].rating_count, Some(16));
}

#[test]
fn search_error_is_reported() {
    let err = parse_search(r#"{"errors":[{"message":"boom","type":"FlexibleSearchError"}]}"#)
        .unwrap_err();
    assert!(err.to_string().contains("boom"));
    assert!(parse_search("<html>maintenance</html>").is_err());
    assert!(parse_search("{}").unwrap().items.is_empty());
}

#[test]
fn item_counter() {
    let p = parse_item(&fixture("item_counter.json")).unwrap();
    assert_eq!(p.id, "2110100000300s");
    assert_eq!(p.name, "セラミック天板 バーカウンター (幅150cm GY)");
    assert_eq!(
        p.url,
        "https://www.nitori-net.jp/ec/product/2110100000300s/"
    );
    assert_eq!(p.brand.as_deref(), Some("ニトリ"));
    assert_eq!(p.price, Some(64990));
    assert_eq!(p.rating, Some(2.0));
    assert_eq!(p.rating_count, Some(1));
    assert_eq!(
        p.variants,
        [
            "カラー: グレー",
            "カラー: ライトグレー",
            "カラー: ナチュラル"
        ]
    );
    assert_eq!(
        p.categories,
        ["テーブル", "カウンターテーブル・バーカウンター"]
    );
    assert!(p.details.contains(&(
        "素材".to_string(),
        "天板：セラミック/他：繊維板".to_string()
    )));
    assert!(
        p.details
            .contains(&("保証期間".to_string(), "5年".to_string()))
    );
    let d = p.description.unwrap();
    assert!(d.starts_with("●天板と側面は石目柄のグレー色"));
    assert!(!d.contains('<') && !d.contains("※使用している小物等は演出品です。\n\n"));
    assert!(p.image.unwrap().contains("211010000031701.jpg"));
}

#[test]
fn item_sale_and_variants() {
    let p = parse_item(&fixture("item_sale.json")).unwrap();
    assert_eq!(p.price, Some(6291));
    assert_eq!(p.badges, ["期間限定価格"]);
    assert_eq!(p.categories, ["テーブル", "折りたたみテーブル"]);

    let p = parse_item(&fixture("item_dining.json")).unwrap();
    assert_eq!(p.id, "2190100002979s");
    assert_eq!(p.price, Some(64990));
    assert_eq!(p.rating_count, Some(42));
    assert_eq!(
        p.categories,
        ["テーブル", "ダイニングテーブル", "ダイニングテーブル4人用"]
    );
    assert!(p.variants.contains(&"サイズ: 幅165cm".to_string()));
    assert!(
        p.variants
            .contains(&"カラー: ミドルブラウン/グレー".to_string())
    );
}

#[test]
fn item_not_found() {
    let body = r#"{"errors":[{"message":"No Base Product found for code '1'","type":"UnknownIdentifierError"}]}"#;
    let err = parse_item(body).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
    assert!(parse_item("{}").is_err());
    assert!(parse_item("not json").is_err());
}

#[test]
fn ids_and_urls() {
    assert_eq!(
        parse_id("2110100000300s").as_deref(),
        Some("2110100000300s")
    );
    assert_eq!(parse_id(" 8761161 ").as_deref(), Some("8761161"));
    assert_eq!(
        parse_id("https://www.nitori-net.jp/ec/product/5620871-5620872s/?foo=1").as_deref(),
        Some("5620871-5620872s")
    );
    assert_eq!(
        parse_id("https://www.nitori-net.jp/ec/product/2190100002979s").as_deref(),
        Some("2190100002979s")
    );
    assert_eq!(parse_id(""), None);
    assert_eq!(parse_id("../../etc"), None);
    assert_eq!(parse_id("abc"), None);
    assert_eq!(
        nitori::product_url("8761161s"),
        "https://www.nitori-net.jp/ec/product/8761161s/"
    );
}

#[test]
fn search_url_params() {
    let u = url::Url::parse(&search_url(&query("毛布 冬", 3, Some(10), Sort::PriceAsc)).unwrap())
        .unwrap();
    let get = |k: &str| {
        u.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.to_string())
    };
    assert_eq!(u.path(), "/occ/v2/nitorinet/nitori/products/search");
    assert_eq!(get("query").as_deref(), Some("毛布 冬"));
    assert_eq!(get("sort").as_deref(), Some("price-asc"));
    assert_eq!(get("currentPage").as_deref(), Some("2"));
    assert_eq!(get("pageSize").as_deref(), Some("10"));
    assert_eq!(get("curr").as_deref(), Some("JPY"));
}

#[test]
fn sort_mapping() {
    assert_eq!(nitori::sort_param(Sort::Relevance), "relevance");
    assert_eq!(nitori::sort_param(Sort::PriceDesc), "price-desc");
    assert_eq!(nitori::sort_param(Sort::Reviews), "ratingCount-desc");
    assert_eq!(nitori::sort_param(Sort::Newest), "effective_date-desc");
    let u = search_url(&query("a:b", 0, None, Sort::Relevance)).unwrap();
    assert!(u.contains("currentPage=0") && u.contains("query=a+b"));
}

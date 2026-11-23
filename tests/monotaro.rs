use shop::{
    fetch::FetchError,
    monotaro::{Id, parse_id, parse_item, parse_search, search_url, ship_label},
    product::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/monotaro/{name}",
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

fn detail<'a>(details: &'a [(String, String)], k: &str) -> Option<&'a str> {
    details
        .iter()
        .find(|(dk, _)| dk == k)
        .map(|(_, v)| v.as_str())
}

#[test]
fn search_fixture() {
    let page = parse_search(&fixture("search.html")).unwrap();
    assert_eq!(page.total, Some(30007));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 3);
    let first = &page.items[0];
    assert_eq!(first.id, "00005036");
    assert_eq!(first.url, "https://www.monotaro.com/g/00005036/");
    assert_eq!(
        first.name,
        "ボールグリップドライバー 単品 ソフト樹脂 刃先マグネットあり"
    );
    assert_eq!(first.brand.as_deref(), Some("ベッセル"));
    assert_eq!(first.price, Some(395), "tax-included price");
    assert_eq!(first.rating, Some(4.73));
    assert_eq!(first.rating_count, Some(177));
    assert_eq!(first.badges, ["当日出荷"]);
    assert_eq!(detail(&first.details, "バリエーション"), Some("18点"));
    assert_eq!(detail(&first.details, "注文コード"), Some("02197517"));
    assert_eq!(detail(&first.details, "品番"), Some("220 +1X75"));
    assert_eq!(
        first.categories.last().map(String::as_str),
        Some("グリップドライバー")
    );
    assert!(
        first
            .image
            .as_deref()
            .unwrap()
            .starts_with("https://jp.images-monotaro.com/")
    );

    let single = &page.items[1];
    assert_eq!(single.price, Some(824));
    assert_eq!(
        detail(&single.details, "品番"),
        None,
        "\"-\" means no part number"
    );
    assert_eq!(detail(&single.details, "バリエーション"), None);
    assert_eq!(detail(&single.details, "内容量"), Some("1セット(6本組)"));
    assert_eq!(detail(&page.items[2].details, "注文コード"), None);
}

#[test]
fn search_broken() {
    assert!(parse_search("<html><body>maintenance</body></html>").is_err());
}

#[test]
fn group_page_lists_variants() {
    let p = parse_item(&fixture("group.html"), &Id::Group("00005028".into())).unwrap();
    assert_eq!(p.id, "00005028");
    assert_eq!(p.url, "https://www.monotaro.com/g/00005028/");
    assert_eq!(p.name, "強力貫通ドライバー");
    assert_eq!(p.brand.as_deref(), Some("ベッセル"));
    assert_eq!(p.price, Some(681), "cheapest variant");
    assert_eq!(p.rating, Some(4.73));
    assert_eq!(p.rating_count, Some(30));
    assert_eq!(p.in_stock, Some(true));
    assert_eq!(p.variants.len(), 4);
    assert_eq!(
        detail(&p.details, "02196704"),
        Some("品番 100 +1X75 | 軸長75mm先端サイズNo.1 | ¥681 | 当日出荷")
    );
    assert_eq!(
        detail(&p.details, "02196731"),
        Some("品番 100 +3X150 | 軸長150mm先端サイズNo.3 | ¥1208 | 当日出荷")
    );
    assert_eq!(p.categories.len(), 5);
}

#[test]
fn item_page_has_part_number_and_specs() {
    let p = parse_item(&fixture("item.html"), &Id::Item("02196704".into())).unwrap();
    assert_eq!(p.id, "02196704");
    assert_eq!(p.url, "https://www.monotaro.com/p/0219/6704/");
    assert_eq!(
        p.name,
        "強力貫通ドライバー プラス 単品 木製 軸長75mm先端サイズNo.1"
    );
    assert_eq!(p.price, Some(681));
    assert_eq!(p.list_price, Some(1111));
    assert_eq!(p.in_stock, Some(true));
    assert_eq!(detail(&p.details, "品番"), Some("100 +1X75"));
    assert_eq!(detail(&p.details, "出荷目安"), Some("当日出荷"));
    assert_eq!(detail(&p.details, "トラスコ品番"), Some("124-7034"));
    assert_eq!(detail(&p.details, "全長(mm)"), Some("182"));
    assert!(
        p.details.iter().all(|(_, v)| !v.contains('<')),
        "catalog links are dropped"
    );
}

#[test]
fn page_without_product_data_is_not_found() {
    let err = parse_item("<html><body>404</body></html>", &Id::Group("1".into())).unwrap_err();
    assert!(matches!(
        err.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
}

#[test]
fn ids() {
    assert_eq!(parse_id("00005028"), Some(Id::Group("00005028".into())));
    assert_eq!(parse_id("g00005028"), Some(Id::Group("00005028".into())));
    assert_eq!(
        parse_id("https://www.monotaro.com/g/00005028/?t.q=x"),
        Some(Id::Group("00005028".into()))
    );
    assert_eq!(
        parse_id("https://www.monotaro.com/p/0219/6704/"),
        Some(Id::Item("02196704".into()))
    );
    assert_eq!(
        Id::Item("02196704".into()).url(),
        "https://www.monotaro.com/p/0219/6704/"
    );
    assert_eq!(parse_id("5028"), None);
    assert_eq!(parse_id("https://www.monotaro.com/p/0219/"), None);
}

#[test]
fn urls_and_labels() {
    assert_eq!(
        search_url(&query("ドライバー", 1, Sort::Relevance)).unwrap(),
        "https://www.monotaro.com/s/?q=%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC"
    );
    assert_eq!(
        search_url(&query("x", 2, Sort::PriceDesc)).unwrap(),
        "https://www.monotaro.com/s/?q=x&sort=-price&page=2"
    );
    assert!(
        search_url(&query("x", 1, Sort::PriceAsc))
            .unwrap()
            .ends_with("sort=price")
    );
    assert!(
        search_url(&query("x", 1, Sort::Reviews))
            .unwrap()
            .ends_with("sort=review_count")
    );
    assert!(search_url(&query("x", 1, Sort::Newest)).is_err());
    assert_eq!(ship_label(0), "当日出荷");
    assert_eq!(ship_label(1), "翌日以内出荷");
    assert_eq!(ship_label(4), "4日以内出荷");
}

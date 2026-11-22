use serde_json::Value;
use shop::{
    fetch::FetchError,
    tablecheck::{
        Destination, Keyword, booking_url, destination_url, is_challenge, parse_destination,
        parse_id, parse_item, parse_search, parse_search_with, resolve_keyword, search_url,
    },
    venue::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/tablecheck/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn catalog() -> Value {
    serde_json::from_str(&fixture("catalog.json")).unwrap()
}

fn query(text: &str, page: u32, limit: Option<u32>, sort: Sort) -> Query<'_> {
    Query {
        text,
        area: None,
        page,
        limit,
        sort,
    }
}

fn params(url: &str) -> Vec<(String, String)> {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn param(url: &str, key: &str) -> Option<String> {
    params(url)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

#[test]
fn keyword_resolves_cuisines_via_catalog() {
    let c = catalog();
    assert_eq!(
        resolve_keyword("寿司", &c),
        Keyword::Cuisines(vec!["sushi".into()])
    );
    assert_eq!(
        resolve_keyword("Sushi", &c),
        Keyword::Cuisines(vec!["sushi".into()])
    );
    assert_eq!(
        resolve_keyword("すし　天ぷら", &c),
        Keyword::Cuisines(vec!["sushi".into(), "tempura".into()])
    );
    assert_eq!(resolve_keyword("  ", &c), Keyword::Any);
    assert_eq!(
        resolve_keyword("おじま", &c),
        Keyword::Name("おじま".into())
    );
    assert_eq!(
        resolve_keyword("寿司 おじま", &c),
        Keyword::Name("寿司 おじま".into())
    );
}

#[test]
fn search_url_for_cuisine() {
    let q = query("寿司", 2, Some(5), Sort::Relevance);
    let url = search_url(
        &q,
        &Keyword::Cuisines(vec!["sushi".into(), "tempura".into()]),
        Some("area:4859036"),
    )
    .unwrap();
    assert!(
        url.starts_with("https://search-v4.production.ap-northeast-1.aws.tablecheck.net/search/?")
    );
    assert_eq!(param(&url, "queryProfile").as_deref(), Some("venue-search"));
    assert_eq!(param(&url, "destination").as_deref(), Some("area:4859036"));
    assert_eq!(param(&url, "cuisine").as_deref(), Some("sushi tempura"));
    assert_eq!(param(&url, "party_size").as_deref(), Some("2"));
    assert_eq!(param(&url, "sort").as_deref(), Some("recommended"));
    assert_eq!(param(&url, "hits").as_deref(), Some("5"));
    assert_eq!(param(&url, "offset").as_deref(), Some("5"));

    let url = search_url(&query("", 0, None, Sort::Relevance), &Keyword::Any, None).unwrap();
    assert_eq!(param(&url, "destination").as_deref(), Some("country:jp"));
    assert_eq!(param(&url, "cuisine").as_deref(), Some("any"));
    assert_eq!(param(&url, "hits").as_deref(), Some("20"));
    assert_eq!(param(&url, "offset").as_deref(), Some("0"));

    let url = search_url(
        &query("", 1, Some(500), Sort::Relevance),
        &Keyword::Any,
        None,
    )
    .unwrap();
    assert_eq!(param(&url, "hits").as_deref(), Some("20"));
    assert!(search_url(&query("", 60, None, Sort::Relevance), &Keyword::Any, None).is_err());
}

#[test]
fn search_url_for_name_lookup() {
    let kw = Keyword::Name("おじま".into());
    let url = search_url(&query("おじま", 2, Some(5), Sort::Relevance), &kw, None).unwrap();
    assert_eq!(param(&url, "queryProfile").as_deref(), Some("venue-lookup"));
    assert_eq!(param(&url, "name").as_deref(), Some("おじま"));
    assert_eq!(param(&url, "destination").as_deref(), Some("country:jp"));
    assert_eq!(param(&url, "hits").as_deref(), Some("10"));
    assert_eq!(param(&url, "offset"), None);
    let err = search_url(&query("おじま", 3, Some(10), Sort::Relevance), &kw, None).unwrap_err();
    assert!(err.to_string().contains("at most 20"));
}

#[test]
fn unsupported_sorts_are_rejected() {
    for sort in [Sort::Rating, Sort::Reviews, Sort::Newest] {
        let err = search_url(&query("寿司", 1, None, sort), &Keyword::Any, None).unwrap_err();
        assert!(err.to_string().contains("does not support --sort"), "{err}");
    }
}

#[test]
fn destination_lookup() {
    let url = destination_url(" 銀座 ").unwrap();
    assert_eq!(
        param(&url, "queryProfile").as_deref(),
        Some("destination-suggest")
    );
    assert_eq!(param(&url, "q").as_deref(), Some("銀座"));
    assert_eq!(param(&url, "country").as_deref(), Some("JP"));
    assert_eq!(
        parse_destination(&fixture("destination.json")).unwrap(),
        Some(Destination {
            id: "area:4859036".into(),
            name: "銀座".into()
        })
    );
    let empty = r#"{"root":{"id":"toplevel","fields":{"totalCount":0}}}"#;
    assert_eq!(parse_destination(empty).unwrap(), None);
}

#[test]
fn search_fixture() {
    let c = catalog();
    let page = parse_search_with(&fixture("search.json"), Some(&c)).unwrap();
    assert_eq!(page.total, Some(64));
    assert_eq!(page.items.len(), 3);
    let v = &page.items[0];
    assert_eq!(v.id, "sushi-ojima");
    assert_eq!(v.name, "銀座鮨おじま");
    assert_eq!(v.url, "https://www.tablecheck.com/ja/sushi-ojima");
    assert_eq!(
        v.booking_url.as_deref(),
        Some("https://www.tablecheck.com/ja/shops/sushi-ojima/reserve")
    );
    assert_eq!(v.genre, ["鮨", "懐石・会席料理"]);
    assert_eq!(v.area.as_deref(), Some("銀座"));
    assert_eq!(v.address.as_deref(), Some("東京都中央区銀座7-8-1"));
    assert_eq!(v.access.as_deref(), Some("銀座駅 徒歩5分 (355m)"));
    assert_eq!(
        v.budget.as_deref(),
        Some("ランチ ￥15,000 / ディナー ￥30,000")
    );
    assert_eq!(
        v.description.as_deref(),
        Some("伝統を守り追随を追い求める真摯な江戸前鮨")
    );
    assert!(
        v.image
            .as_deref()
            .unwrap()
            .ends_with("4463ce54.jpg?1674118046")
    );
    assert_eq!(page.items[1].id, "ginza-happo");
    assert_eq!(page.items[2].budget.as_deref(), Some("ディナー ￥30,000"));

    let raw = parse_search(&fixture("search.json")).unwrap();
    assert_eq!(raw.items[0].genre, ["sushi", "kaiseki"]);
}

#[test]
fn lookup_fixture() {
    let page = parse_search(&fixture("lookup.json")).unwrap();
    assert_eq!(page.total, Some(48));
    let names: Vec<&str> = page.items.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["鮨もりなり", "寿司しおの"]);
    assert_eq!(page.items[0].area.as_deref(), Some("築地"));
}

#[test]
fn search_errors_are_reported() {
    let err = parse_search(r#"{"error":"party_size is required"}"#).unwrap_err();
    assert!(err.to_string().contains("party_size is required"));
    let err = parse_search(
        r#"{"root":{"id":"toplevel","fields":{"totalCount":0},"errors":[{"code":3,"summary":"Illegal query","message":"20 hits max"}]}}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("20 hits max"));
    assert!(parse_search("<html>maintenance</html>").is_err());
    assert!(parse_search("{}").is_err());
    assert!(parse_search(r#"{"root":{"id":"toplevel"}}"#).is_err());
    let none = parse_search(r#"{"root":{"id":"toplevel","fields":{"totalCount":0}}}"#).unwrap();
    assert_eq!(none.total, Some(0));
    assert!(none.items.is_empty());
}

#[test]
fn item_fixture() {
    let c = catalog();
    let v = parse_item(&fixture("item.html"), Some(&c)).unwrap();
    assert_eq!(v.id, "sushi-ojima");
    assert_eq!(v.name, "銀座鮨おじま");
    assert_eq!(v.url, "https://www.tablecheck.com/ja/sushi-ojima");
    assert_eq!(
        v.booking_url.as_deref(),
        Some("https://www.tablecheck.com/ja/shops/sushi-ojima/reserve")
    );
    assert_eq!(v.genre, ["鮨", "懐石・会席料理"]);
    assert_eq!(v.area.as_deref(), Some("中央区"));
    assert_eq!(
        v.address.as_deref(),
        Some("〒104-0061 東京都中央区銀座7-8-1 銀座梅林ビル 8F")
    );
    assert_eq!(v.access.as_deref(), Some("銀座駅 (325m) / 東銀座駅 (451m)"));
    assert_eq!(
        v.budget.as_deref(),
        Some("ランチ ￥10,000～￥20,000 / ディナー ￥30,000")
    );
    assert_eq!(v.phone.as_deref(), Some("03-6228-5957"));
    assert_eq!(v.hours, None);
    assert_eq!(v.rating, None);
    assert!(v.image.as_deref().unwrap().contains("4463ce54.jpg"));
    let d = v.description.as_deref().unwrap();
    assert!(d.starts_with("伝統を守り追随を追い求める真摯な江戸前鮨\nシャリとネタ"));
    assert!(!d.contains('\r'));
    assert!(
        v.details
            .contains(&("読み".into(), "ギンザスシオジマ".into()))
    );
    assert!(
        v.details
            .contains(&("席".into(), "カウンター（1-12名）、和個室（2-8名）".into()))
    );
    assert!(
        v.details
            .contains(&("ウェブサイト".into(), "https://sushi-ojima.jp/".into()))
    );
    assert!(v.details.iter().any(|(k, val)| k == "特徴"
        && val.contains("接待・ビジネス")
        && !val.contains("smokefree")));
    assert!(!v.menus[0].description.as_deref().unwrap().contains('\n'));

    assert_eq!(v.menus.len(), 3);
    let m = &v.menus[0];
    assert_eq!(m.name, "【ランチ】百合（お昼のみ）");
    assert_eq!(m.price, Some(22000));
    assert_eq!(m.price_text.as_deref(), Some("￥22,000（税込）"));
    assert!(
        m.description
            .as_deref()
            .unwrap()
            .starts_with("清らかで凛とした")
    );
    assert_eq!(v.menus[1].price, Some(33000));
    assert_eq!(v.menus[2].price, Some(49500));
}

#[test]
fn item_without_catalog_keeps_keys() {
    let v = parse_item(&fixture("item.html"), None).unwrap();
    assert_eq!(v.genre, ["sushi", "kaiseki"]);
}

#[test]
fn item_operating_days() {
    let html = r#"<script>window.__REACT_QUERY_STATE__={"mutations":[],"queries":[{"queryKey":["venue","x","ja"],"state":{"data":{"shops":[{"slug":"x","name_translations":[{"translation":"店","locale":"ja"}],"operating_days":{"sunday":"closed","monday":{"timeslots":[{"slot":"lunch","opening_time":"11:30","closing_time":"14:00","food_lo":"13:30"},{"slot":"dinner","opening_time":"17:00","closing_time":"22:00"}]},"tuesday":{"timeslots":[]}}}]}}}]}</script>"#;
    let v = parse_item(html, None).unwrap();
    assert_eq!(v.name, "店");
    assert_eq!(
        v.hours.as_deref(),
        Some("月 11:30-14:00 (LO 13:30), 17:00-22:00")
    );
    assert_eq!(v.closed.as_deref(), Some("火、日"));
    assert_eq!(v.booking_url.as_deref(), Some(&*booking_url("x")));
}

#[test]
fn item_not_found() {
    let err = parse_item(&fixture("not_found.html"), None).unwrap_err();
    assert!(matches!(err.downcast_ref(), Some(FetchError::NotFound(_))));
    let empty = r#"<script>window.__REACT_QUERY_STATE__={"queries":[{"queryKey":["venue","x","ja"],"state":{"data":{"shops":[]}}}]}</script>"#;
    let err = parse_item(empty, None).unwrap_err();
    assert!(matches!(err.downcast_ref(), Some(FetchError::NotFound(_))));
}

#[test]
fn item_unrecognised_page_is_an_error() {
    let err = parse_item("<html><body>hello</body></html>", None).unwrap_err();
    assert!(err.downcast_ref::<FetchError>().is_none());
    assert!(err.to_string().contains("unrecognised"));
}

#[test]
fn ids() {
    for input in [
        "sushi-ojima",
        " sushi-ojima ",
        "https://www.tablecheck.com/ja/sushi-ojima",
        "https://www.tablecheck.com/ja/shops/sushi-ojima/reserve",
        "https://www.tablecheck.com/en/shops/sushi-ojima/reserve?menu_items=1#x",
        "https://www.tablecheck.com/zh-CN/sushi-ojima",
        "https://www.tablecheck.com/shops/sushi-ojima/reserve",
        "www.tablecheck.com/ja/sushi-ojima/",
        "/ja/sushi-ojima",
    ] {
        assert_eq!(parse_id(input).as_deref(), Some("sushi-ojima"), "{input}");
    }
    assert_eq!(
        parse_id("ginza_hachigou").as_deref(),
        Some("ginza_hachigou")
    );
    for bad in [
        "",
        "寿司",
        "https://www.tablecheck.com/ja/japan",
        "https://www.tablecheck.com/ja/japan/search?cuisines[]=sushi",
        "https://www.tablecheck.com/ja/",
        "sushi ojima",
    ] {
        assert_eq!(parse_id(bad), None, "{bad}");
    }
}

#[test]
fn challenge_detection() {
    let page = "ド".repeat(3000);
    assert!(!is_challenge(&page));
    assert!(!is_challenge(&fixture("item.html")));
    assert!(is_challenge(
        "<html><head><title>Just a moment...</title></head></html>"
    ));
}

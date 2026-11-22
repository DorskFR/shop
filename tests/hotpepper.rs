use shop::{
    fetch::FetchError,
    hotpepper::{is_blocked, parse_id, parse_item, parse_menus, parse_search, search_url},
    venue::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/hotpepper/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query<'a>(text: &'a str, area: Option<&'a str>, page: u32, sort: Sort) -> Query<'a> {
    Query {
        text,
        area,
        page,
        limit: None,
        sort,
    }
}

fn fetch_error(e: &anyhow::Error) -> Option<&FetchError> {
    e.downcast_ref::<FetchError>()
}

#[test]
fn search_page() {
    let page = parse_search(&fixture("search.html")).unwrap();
    assert_eq!(page.total, Some(2447));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 3);

    let zina = &page.items[0];
    assert_eq!(zina.id, "H000660218");
    assert_eq!(zina.url, "https://beauty.hotpepper.jp/slnH000660218/");
    assert_eq!(
        zina.name,
        "Zina 渋谷神南 髪質改善/前髪カット/レイヤーカット【ジーナ】"
    );
    assert_eq!(zina.genre, ["ヘアサロン"]);
    assert_eq!(zina.budget.as_deref(), Some("カット ￥1,100～"));
    assert_eq!(zina.rating, None);
    assert_eq!(zina.rating_count, Some(1273));
    assert!(zina.access.as_deref().unwrap().contains("Appleストア隣"));
    assert_eq!(
        zina.details,
        [("席数".to_string(), "セット面17席".to_string())]
    );
    assert_eq!(
        zina.booking_url.as_deref(),
        Some("https://beauty.hotpepper.jp/CSP/bt/reserve/?storeId=H000660218")
    );
    assert!(
        zina.image
            .as_deref()
            .unwrap()
            .starts_with("https://imgbp.hotp.jp/")
    );
    assert!(zina.description.is_some());
    assert_eq!(zina.menus.len(), 3);
    assert_eq!(zina.menus[0].price, Some(15900));
    assert_eq!(zina.menus[0].price_text.as_deref(), Some("￥15,900"));
    assert!(
        zina.menus[0]
            .description
            .as_deref()
            .unwrap()
            .starts_with('[')
    );

    let fm = &page.items[2];
    assert_eq!(fm.id, "H000684793");
    assert_eq!(fm.genre, ["メンズサロン"]);
    assert_eq!(fm.rating_count, Some(366));
    assert_eq!(fm.menus[0].price, Some(4000));
}

#[test]
fn search_empty_and_unknown_layout() {
    let page = parse_search(&fixture("search_empty.html")).unwrap();
    assert_eq!(page.total, Some(0));
    assert!(page.items.is_empty());

    let err = parse_search("<html><body><p>maintenance</p></body></html>").unwrap_err();
    assert!(err.to_string().contains("not recognised"));
}

#[test]
fn blocked_pages() {
    let akamai = "<html><head><title>Access Denied</title></head><body>Reference #18</body></html>";
    assert!(is_blocked(akamai).is_some());
    let err = parse_search(akamai).unwrap_err();
    assert!(matches!(fetch_error(&err), Some(FetchError::Blocked(_))));
    let err = parse_item(akamai, None).unwrap_err();
    assert!(matches!(fetch_error(&err), Some(FetchError::Blocked(_))));
    for f in ["search.html", "item.html", "coupon.html", "missing.html"] {
        assert!(is_blocked(&fixture(f)).is_none(), "{f}");
    }
}

#[test]
fn item_page() {
    let v = parse_item(&fixture("item.html"), Some(&fixture("coupon.html"))).unwrap();
    assert_eq!(v.id, "H000419288");
    assert_eq!(v.name, "Lomalia 渋谷宮益坂【ロマリア】");
    assert!(v.badges.is_empty());
    assert_eq!(v.url, "https://beauty.hotpepper.jp/slnH000419288/");
    assert_eq!(v.genre, ["ヘアサロン"]);
    assert_eq!(v.area.as_deref(), Some("渋谷"));
    assert_eq!(v.rating, Some(4.89));
    assert_eq!(v.rating_count, Some(844));
    assert_eq!(v.phone.as_deref(), Some("03-6427-7272"));
    assert_eq!(
        v.address.as_deref(),
        Some("東京都渋谷区渋谷１-5-12 山崎ビル3F")
    );
    assert!(v.access.as_deref().unwrap().starts_with("渋谷駅B3番出口"));
    assert!(!v.access.as_deref().unwrap().ends_with(']'));
    assert_eq!(
        v.hours.as_deref(),
        Some("平日/11:00～20:00土曜日・日曜日・祝日/10:00～19:00")
    );
    assert_eq!(v.closed.as_deref(), Some("月曜日（祝日の場合は翌日）"));
    assert_eq!(v.budget.as_deref(), Some("カット ￥5,000～"));
    assert_eq!(
        v.booking_url.as_deref(),
        Some("https://beauty.hotpepper.jp/CSP/bt/reserve/?storeId=H000419288")
    );
    assert!(v.image.as_deref().unwrap().contains("B262275535"));
    let d = v.description.as_deref().unwrap();
    assert!(d.starts_with("【仕事帰り当日予約◎】"));
    assert!(d.contains("専属美容師"));

    let detail = |k: &str| {
        v.details
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(detail("席数"), Some("セット面5席"));
    assert_eq!(
        detail("スタッフ数"),
        Some("スタイリスト4人／アシスタント1人")
    );
    assert_eq!(detail("駐車場"), Some("近隣にコインパーキング有"));
    assert!(detail("支払い方法").unwrap().starts_with("Visa"));
    assert!(detail("住所").is_none());

    assert_eq!(v.menus.len(), 6);
    let c = &v.menus[0];
    assert!(
        c.name
            .starts_with("【平日だけの特別クーポン】カット+カラー")
    );
    assert_eq!(c.price, Some(9500));
    assert_eq!(c.price_text.as_deref(), Some("¥9,500"));
    assert!(
        c.description
            .as_deref()
            .unwrap()
            .starts_with("[新規・平日限定] ★ダメージレス")
    );
    let m = &v.menus[3];
    assert_eq!(
        m.name,
        "カット＋カラー＋プレミアム４stepトリートメント（ホームケア付）"
    );
    assert_eq!(m.price, Some(20500));
    assert_eq!(m.price_text.as_deref(), Some("¥20,500～"));
}

#[test]
fn item_mens_salon() {
    let v = parse_item(&fixture("item_mens.html"), None).unwrap();
    assert_eq!(v.id, "H000684793");
    assert_eq!(
        v.name,
        "FM 渋谷 メンズサロン/メンズカット/メンズパーマ【エフエム】"
    );
    assert_eq!(v.genre, ["メンズサロン"]);
    assert_eq!(v.badges, ["スマート支払いOK"]);
    assert_eq!(v.rating, Some(4.75));
    assert_eq!(v.rating_count, Some(366));
    assert_eq!(
        v.hours.as_deref(),
        Some("平日/11:00～21:00 土日祝日/10:00～20:00")
    );
}

#[test]
fn item_without_coupon_page() {
    let v = parse_item(&fixture("item.html"), None).unwrap();
    assert!(v.menus.is_empty());
    assert_eq!(v.rating, Some(4.89));
}

#[test]
fn menus_only() {
    let menus = parse_menus(&fixture("coupon.html")).unwrap();
    assert_eq!(menus.len(), 6);
    assert!(menus.iter().all(|m| m.price.is_some()));
}

#[test]
fn item_missing_and_unknown_layout() {
    let err = parse_item(&fixture("missing.html"), None).unwrap_err();
    assert!(matches!(fetch_error(&err), Some(FetchError::NotFound(_))));
    let err = parse_item("<html><body><p>hello</p></body></html>", None).unwrap_err();
    assert!(err.to_string().contains("not recognised"));
}

#[test]
fn ids() {
    assert_eq!(parse_id("H000419288").as_deref(), Some("H000419288"));
    assert_eq!(parse_id(" h000419288 ").as_deref(), Some("H000419288"));
    assert_eq!(parse_id("slnH000419288").as_deref(), Some("H000419288"));
    assert_eq!(
        parse_id("https://beauty.hotpepper.jp/slnH000419288/").as_deref(),
        Some("H000419288")
    );
    assert_eq!(
        parse_id("https://beauty.hotpepper.jp/slnH000419288/?cstt=1").as_deref(),
        Some("H000419288")
    );
    assert_eq!(
        parse_id("https://beauty.hotpepper.jp/slnH000419288/coupon/").as_deref(),
        Some("H000419288")
    );
    assert_eq!(
        parse_id("https://beauty.hotpepper.jp/CSP/bt/reserve/?storeId=H000419288").as_deref(),
        Some("H000419288")
    );
    assert_eq!(parse_id("419288"), None);
    assert_eq!(parse_id("H"), None);
    assert_eq!(parse_id("https://example.com/slnH000419288/"), None);
    assert_eq!(parse_id("https://beauty.hotpepper.jp/top/"), None);
}

#[test]
fn urls() {
    let u = search_url(&query("カット", Some("渋谷"), 1, Sort::Relevance)).unwrap();
    assert_eq!(
        u,
        "https://beauty.hotpepper.jp/CSP/bt/salonSearch/search/?freeword=%E3%82%AB%E3%83%83%E3%83%88+%E6%B8%8B%E8%B0%B7&searchGender=ALL&sortType=popular"
    );
    let u = search_url(&query("", Some("新宿"), 3, Sort::Relevance)).unwrap();
    assert!(u.contains("freeword=%E6%96%B0%E5%AE%BF&"));
    assert!(u.ends_with("&pn=3"));
    assert!(search_url(&query(" ", None, 1, Sort::Relevance)).is_err());
    for s in [Sort::Rating, Sort::Reviews, Sort::Newest] {
        let err = search_url(&query("カット", None, 1, s)).unwrap_err();
        assert!(err.to_string().contains("does not support --sort"));
    }
}

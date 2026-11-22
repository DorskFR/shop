use shop::{
    fetch::FetchError,
    tabelog::{
        Area, is_challenge, item_url, page_url, parse_id, parse_item, parse_search, parse_suggest,
        search_url, sort_code, suggest_url,
    },
    venue::{Query, Sort},
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/tabelog/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn query(text: &str, sort: Sort) -> Query<'_> {
    Query {
        text,
        area: None,
        page: 1,
        limit: None,
        sort,
    }
}

fn shibuya() -> Area {
    Area {
        name: "渋谷駅".into(),
        datatype: "RailroadStation".into(),
        id: 4698,
    }
}

fn fetch_error(err: &anyhow::Error) -> &FetchError {
    err.downcast_ref::<FetchError>()
        .unwrap_or_else(|| panic!("expected FetchError, got {err}"))
}

#[test]
fn suggest_resolves_station_first() {
    assert_eq!(
        suggest_url("渋谷").unwrap(),
        "https://tabelog.com/internal_api/suggest_form_words?sa=%E6%B8%8B%E8%B0%B7"
    );
    let areas = parse_suggest(&fixture("suggest.json")).unwrap();
    assert_eq!(areas.len(), 5);
    assert_eq!(areas[0], shibuya());
    assert_eq!(areas[1].datatype, "Town");
    assert!(parse_suggest("[]").unwrap().is_empty());
    assert!(parse_suggest("<html>").is_err());
    assert!(parse_suggest("{\"error\":1}").is_err());
}

#[test]
fn search_urls() {
    assert_eq!(
        search_url(&query("ラーメン", Sort::Relevance), None).unwrap(),
        "https://tabelog.com/rst/rstsearch?sk=%E3%83%A9%E3%83%BC%E3%83%A1%E3%83%B3"
    );
    assert_eq!(
        search_url(&query("ラーメン", Sort::Rating), Some(&shibuya())).unwrap(),
        "https://tabelog.com/rst/rstsearch?sk=%E3%83%A9%E3%83%BC%E3%83%A1%E3%83%B3&sa=%E6%B8%8B%E8%B0%B7%E9%A7%85&area_datatype=RailroadStation&area_id=4698&SrtT=rt"
    );
    assert_eq!(sort_code(Sort::Relevance), None);
    assert_eq!(sort_code(Sort::Rating), Some("rt"));
    assert_eq!(sort_code(Sort::Reviews), Some("rvcn"));
    assert_eq!(sort_code(Sort::Newest), Some("nod"));
}

#[test]
fn paging_follows_resolved_area_path() {
    let body = fixture("search.html");
    assert_eq!(
        page_url(&body, 3).as_deref(),
        Some(
            "https://tabelog.com/tokyo/A1303/A130301/R4698/rstLst/ramen/3/?SrtT=rt&sk=%E3%83%A9%E3%83%BC%E3%83%A1%E3%83%B3&Srt=D"
        )
    );
    assert_eq!(page_url(&fixture("search_empty.html"), 2), None);
}

#[test]
fn search_page() {
    let page = parse_search(&fixture("search.html")).unwrap();
    assert_eq!(page.total, Some(135));
    assert_eq!(page.page, 1);
    assert_eq!(page.items.len(), 4);

    let first = &page.items[0];
    assert_eq!(first.id, "13003367");
    assert_eq!(first.name, "はやし");
    assert_eq!(
        first.url,
        "https://tabelog.com/tokyo/A1303/A130301/13003367/"
    );
    assert_eq!(first.genre, ["ラーメン"]);
    assert_eq!(first.area.as_deref(), Some("渋谷駅 360m"));
    assert_eq!(first.rating, Some(3.77));
    assert_eq!(first.rating_count, Some(3612));
    assert_eq!(first.budget.as_deref(), Some("昼 ￥1,000～￥1,999"));
    assert_eq!(first.closed.as_deref(), Some("水曜日、日曜日、祝日"));
    assert_eq!(first.badges, ["ラーメン TOKYO 百名店 2025 選出店"]);
    assert_eq!(first.booking_url, None);
    assert!(
        first
            .image
            .as_deref()
            .unwrap()
            .starts_with("https://tblg.k-img.com/")
    );

    let kiraku = &page.items[1];
    assert_eq!(kiraku.genre, ["ラーメン", "餃子", "中華料理"]);
    assert_eq!(kiraku.area.as_deref(), Some("渋谷駅 408m (神泉駅 355m)"));

    let lature = &page.items[3];
    assert_eq!(lature.name, "ラチュレ");
    assert_eq!(
        lature.budget.as_deref(),
        Some("夜 ￥10,000～￥14,999 / 昼 ￥6,000～￥7,999")
    );
    assert_eq!(
        lature.badges,
        [
            "2026年Bronze受賞店",
            "フレンチ TOKYO 百名店 2025 選出店",
            "ネット予約"
        ]
    );
    assert_eq!(lature.booking_url.as_deref(), Some(lature.url.as_str()));
}

#[test]
fn search_without_results_is_empty_not_error() {
    let page = parse_search(&fixture("search_empty.html")).unwrap();
    assert_eq!(page.total, Some(0));
    assert!(page.items.is_empty());
}

#[test]
fn search_rejects_unknown_layout_and_challenge() {
    let err = parse_search("<html><body><p>maintenance</p></body></html>").unwrap_err();
    assert!(err.to_string().contains("layout not recognised"));
    let challenge = fixture("challenge.html");
    assert!(is_challenge(&challenge));
    let err = parse_search(&challenge).unwrap_err();
    assert!(matches!(fetch_error(&err), FetchError::Blocked(_)));
}

#[test]
fn ids() {
    let path = "tokyo/A1303/A130301/13278405";
    for input in [
        "https://tabelog.com/tokyo/A1303/A130301/13278405/",
        "https://tabelog.com/tokyo/A1303/A130301/13278405/dtlmenu/",
        "https://tabelog.com/en/tokyo/A1303/A130301/13278405/",
        "https://s.tabelog.com/tokyo/A1303/A130301/13278405/?lid=x",
        "tokyo/A1303/A130301/13278405",
        "/tokyo/A1303/A130301/13278405/",
    ] {
        assert_eq!(parse_id(input).as_deref(), Some(path), "{input}");
    }
    assert_eq!(
        parse_id("26030764").as_deref(),
        Some("kyoto/A2601/A260101/26030764")
    );
    assert_eq!(
        parse_id("13278405").as_deref(),
        Some("tokyo/A1301/A130101/13278405")
    );
    for bad in [
        "",
        "abc",
        "99123456",
        "https://example.com/tokyo/A1303/A130301/13278405/",
        "https://tabelog.com/rstLst/ramen/",
        "https://tabelog.com/tokyo/A1303/A130301/R4698/rstLst/",
        "atlantis/A1303/A130301/13278405",
    ] {
        assert_eq!(parse_id(bad), None, "{bad}");
    }
    assert_eq!(
        item_url(path),
        "https://tabelog.com/tokyo/A1303/A130301/13278405/"
    );
}

#[test]
fn item_hyakumeiten() {
    let v = parse_item(&fixture("item_hyakumeiten.html")).unwrap();
    assert_eq!(v.id, "13003367");
    assert_eq!(v.name, "はやし");
    assert_eq!(v.url, "https://tabelog.com/tokyo/A1303/A130301/13003367/");
    assert_eq!(v.genre, ["ラーメン"]);
    assert_eq!(v.area.as_deref(), Some("渋谷駅"));
    assert_eq!(
        v.address.as_deref(),
        Some("東京都渋谷区道玄坂1-14-9 ソシアル道玄坂 1F")
    );
    let access = v.access.as_deref().unwrap();
    assert!(access.starts_with("京王井の頭線【渋谷駅】西口 徒歩２分\n"));
    assert!(access.ends_with("\n渋谷駅から360m"));
    assert_eq!(v.rating, Some(3.77));
    assert_eq!(v.rating_count, Some(3612));
    assert_eq!(v.budget.as_deref(), Some("昼 ￥1,000～￥1,999"));
    assert_eq!(
        v.hours.as_deref(),
        Some("月・火・木・金・土 11:30 - 15:30\nスープ無くなり次第閉店")
    );
    assert_eq!(v.closed.as_deref(), Some("水・日・祝日"));
    assert_eq!(v.phone.as_deref(), Some("03-3770-9029"));
    assert_eq!(v.booking_url, None);
    assert_eq!(v.badges, ["ラーメン 百名店 2025 選出店"]);
    let detail = |k: &str| {
        v.details
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, d)| d.as_str())
    };
    assert_eq!(detail("席数"), Some("10席 （カウンターのみ）"));
    assert_eq!(detail("予約可否"), Some("予約不可"));
    assert_eq!(
        detail("予算（口コミ集計）"),
        Some("夜 ￥1,000～￥1,999 / 昼 ￥1,000～￥1,999")
    );
    assert_eq!(detail("オープン日"), Some("2003年11月11日"));
    for skipped in ["店名", "住所", "営業時間", "最近の編集者", "初投稿者"] {
        assert_eq!(detail(skipped), None, "{skipped}");
    }
    assert!(
        v.details
            .iter()
            .all(|(_, d)| !d.contains("を見る") && !d.contains("ご確認ください"))
    );
    assert!(v.menus.is_empty());
    assert_eq!(v.description, None);
    assert!(v.image.as_deref().unwrap().contains("640x640_rect"));
}

#[test]
fn item_with_booking_courses_and_pr() {
    let v = parse_item(&fixture("item_booking.html")).unwrap();
    assert_eq!(v.id, "13198692");
    assert_eq!(v.name, "ラチュレ");
    assert_eq!(v.genre, ["フレンチ"]);
    assert_eq!(
        v.budget.as_deref(),
        Some("夜 ￥10,000～￥14,999 / 昼 ￥6,000～￥7,999")
    );
    assert_eq!(
        v.hours.as_deref(),
        Some(
            "月・火・水・木・金・土・日・祝日・祝前日・祝後日 11:30 - 15:30 (L.O. 13:30), 18:00 - 23:00 (L.O. 20:00)"
        )
    );
    assert_eq!(
        v.closed.as_deref(),
        Some("不定休（HPまたは食べログ備考欄に記載しております）")
    );
    assert_eq!(v.phone.as_deref(), Some("03-6450-5297"));
    assert_eq!(v.booking_url.as_deref(), Some(v.url.as_str()));
    assert_eq!(
        v.badges,
        [
            "2026年Bronze受賞店",
            "2022年Silver受賞店",
            "フレンチ 百名店 2025 選出店"
        ]
    );
    let detail = |k: &str| {
        v.details
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, d)| d.as_str())
    };
    assert_eq!(detail("貸切"), Some("可 （20人以下可、20人～50人可）"));
    assert!(detail("支払い方法").unwrap().starts_with("カード可 （VISA"));
    assert_eq!(detail("ホームページ"), Some("https://www.lature.jp"));

    assert_eq!(v.menus.len(), 2);
    let m = &v.menus[0];
    assert_eq!(m.name, "Menu LATURE PLUS〈ランチスペシャルコース〉");
    assert_eq!(m.price, Some(10000));
    assert_eq!(m.price_text.as_deref(), Some("10,000円 (税込)"));
    assert!(m.description.as_deref().unwrap().starts_with("6品 / "));
    assert_eq!(v.menus[1].price, Some(7800));

    let d = v.description.as_deref().unwrap();
    assert!(d.starts_with("シェフの想いの込もった料理とおもてなしで幸せなひと時を‥\nラチュレとは"));
    assert!(d.ends_with("25日 １部◯ ２部△"));
    assert!(!d.contains("続きを読む"));
}

#[test]
fn item_sparse() {
    let v = parse_item(&fixture("item_sparse.html")).unwrap();
    assert_eq!(v.id, "13278405");
    assert_eq!(v.name, "渋谷苑");
    assert_eq!(v.rating, Some(3.01));
    assert_eq!(v.rating_count, Some(3));
    assert_eq!(v.phone, None);
    assert_eq!(v.budget, None);
    assert_eq!(v.hours, None);
    assert_eq!(v.closed, None);
    assert!(v.badges.is_empty());
    assert_eq!(v.access.as_deref(), Some("渋谷駅から253m"));
    let smoking = v.details.iter().find(|(k, _)| k == "禁煙・喫煙").unwrap();
    assert_eq!(smoking.1, "全席喫煙可");
}

#[test]
fn item_errors() {
    let err = parse_item(&fixture("not_found.html")).unwrap_err();
    assert!(matches!(fetch_error(&err), FetchError::NotFound(_)));
    let err = parse_item(&fixture("challenge.html")).unwrap_err();
    assert!(matches!(fetch_error(&err), FetchError::Blocked(_)));
    let err = parse_item(&fixture("search.html")).unwrap_err();
    assert!(err.to_string().contains("layout not recognised"));
}

#[test]
fn item_hours_without_day_label() {
    let html = r#"<html><head><link rel="canonical" href="https://tabelog.com/kyoto/A2601/A260301/26030764/"></head><body>
<h2 class="display-name"><span>道人</span></h2>
<div id="rst-data-head"><table class="c-table c-table--form rstinfo-table__table"><tbody>
<tr><th>営業時間</th><td>
<ul class="rstinfo-table__business-list"><li class="rstinfo-table__business-item">
<ul class="rstinfo-table__business-dtl is-nolabel"><li class="rstinfo-table__business-dtl-text ">18:00 - 22:00</li></ul>
</li></ul>
<div class="rstinfo-table__business-other"><ul class="rstinfo-table__business-list">
<li class="rstinfo-table__business-item">■ 定休日<br />不定休</li></ul></div>
</td></tr></tbody></table></div></body></html>"#;
    let v = parse_item(html).unwrap();
    assert_eq!(v.id, "26030764");
    assert_eq!(v.hours.as_deref(), Some("18:00 - 22:00"));
    assert_eq!(v.closed.as_deref(), Some("不定休"));
}

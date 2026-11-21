use shop::{
    amazon::{self, Market, PageKind, classify},
    rakuten,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn jp_search() {
    let r = amazon::parse_search(&fixture("jp_search.html"), Market::Jp);
    assert_eq!(r.len(), 60);
    assert!(r.iter().all(|x| x.asin.len() == 10 && !x.title.is_empty()));
    assert!(r.iter().filter(|x| x.price_value.is_some()).count() >= 55);
    assert!(r.iter().any(|x| x.sponsored) && r.iter().any(|x| !x.sponsored));
    let aokimi = r
        .iter()
        .find(|x| x.asin == "B0CT857V89" && !x.sponsored)
        .unwrap();
    assert_eq!(aokimi.price.as_deref(), Some("￥1,963"));
    assert_eq!(aokimi.price_value, Some(1963.0));
    assert_eq!(aokimi.list_price.as_deref(), Some("￥3,276"));
    assert_eq!(aokimi.rating, Some(4.3));
    assert_eq!(aokimi.rating_count, Some(5930));
    assert_eq!(aokimi.url, "https://www.amazon.co.jp/dp/B0CT857V89");
    assert!(
        aokimi
            .bought_past_month
            .as_deref()
            .unwrap()
            .contains("購入")
    );
    assert!(aokimi.badges.iter().any(|b| b == "タイムセール"));
}

#[test]
fn com_search() {
    let r = amazon::parse_search(&fixture("com_search.html"), Market::Com);
    assert_eq!(r.len(), 60);
    let first = &r[0];
    assert_eq!(first.asin, "B0CT857V89");
    assert!(first.sponsored);
    assert_eq!(first.rating, Some(4.3));
    assert!(first.rating_count.unwrap() > 5000);
    assert!(first.badges.iter().any(|b| b == "Amazon's Choice"));
    assert_eq!(first.url, "https://www.amazon.com/dp/B0CT857V89");
}

#[test]
fn jp_item() {
    let i = amazon::parse_item(&fixture("jp_item.html"), Market::Jp, "B0CT857V89");
    assert!(i.title.contains("AOKIMI"));
    assert_eq!(i.price.as_deref(), Some("￥1,963"));
    assert_eq!(i.list_price.as_deref(), Some("￥3,276"));
    assert_eq!(i.availability.as_deref(), Some("在庫あり。"));
    assert_eq!(i.seller.as_deref(), Some("WFQStore"));
    assert_eq!(i.rating, Some(4.3));
    assert_eq!(i.rating_count, Some(5930));
    let h: Vec<(u8, u8)> = i.histogram.iter().map(|r| (r.stars, r.percent)).collect();
    assert_eq!(h, vec![(5, 72), (4, 10), (3, 7), (2, 3), (1, 8)]);
    assert_eq!(i.categories.last().map(String::as_str), Some("イヤホン"));
    assert_eq!(i.features.len(), 6);
    assert!(i.details.iter().any(|(k, v)| k == "型番" && v == "V12-BS"));
    assert!(!i.details.iter().any(|(k, _)| k == "おすすめ度"));
    assert_eq!(i.reviews.len(), 8);
    let r = &i.reviews[0];
    assert_eq!(r.id.as_deref(), Some("R228ECR6VUBYHX"));
    assert_eq!(r.rating, Some(5.0));
    assert_eq!(
        r.title.as_deref(),
        Some("コスパ最高！音質も大満足のイヤホンです")
    );
    assert!(r.verified);
    assert_eq!(r.variant.as_deref(), Some("色: ホワイト-V12"));
    assert!(r.body.starts_with("価格が手頃"));
    assert!(
        i.reviews
            .iter()
            .all(|r| !r.body.is_empty() && r.rating.is_some())
    );
}

#[test]
fn jp_item_out_of_stock() {
    let i = amazon::parse_item(&fixture("jp_item_oos.html"), Market::Jp, "B0D1XD1ZV3");
    assert!(i.title.starts_with("Apple AirPods Pro"));
    assert_eq!(i.price, None);
    assert_eq!(i.availability.as_deref(), Some("現在在庫切れです。"));
    assert_eq!(i.rating_count, Some(46510));
    assert_eq!(i.histogram.len(), 5);
    assert_eq!(i.reviews.len(), 5);
}

#[test]
fn page_classification() {
    assert_eq!(
        classify(&fixture("blocked_captcha.html")),
        PageKind::Blocked("captcha")
    );
    assert_eq!(
        classify(&fixture("blocked_503.html")),
        PageKind::Blocked("error page (503)")
    );
    assert_eq!(classify(&fixture("com_notfound.html")), PageKind::NotFound);
    for f in [
        "jp_search.html",
        "com_search.html",
        "jp_item.html",
        "jp_item_oos.html",
    ] {
        assert_eq!(classify(&fixture(f)), PageKind::Ok, "{f}");
    }
}

#[test]
fn rakuten_formats() {
    let v2 = r#"{"count":2,"page":1,"pageCount":1,"Items":[
        {"itemName":"イヤホン A","itemCode":"shop-a:10001","itemPrice":1980,"itemUrl":"https://item.rakuten.co.jp/shop-a/10001/",
         "shopName":"Shop A","shopCode":"shop-a","reviewAverage":4.25,"reviewCount":120,"availability":1,"postageFlag":0,
         "pointRate":2,"catchcopy":"","itemCaption":"説明","mediumImageUrls":["https://thumbnail.image.rakuten.co.jp/a.jpg?_ex=128x128"]},
        {"itemName":"イヤホン B","itemCode":"shop-b:2","itemPrice":"3500","reviewAverage":"3.5","reviewCount":0,"availability":0,"postageFlag":1}]}"#;
    let p = rakuten::parse_response(200, v2).unwrap();
    assert_eq!((p.count, p.page, p.page_count), (2, 1, 1));
    let a = &p.items[0];
    assert_eq!(a.price, 1980);
    assert_eq!(a.rating, 4.25);
    assert!(a.in_stock && a.postage_included);
    assert_eq!(a.catchcopy, None);
    assert_eq!(
        a.images,
        vec!["https://thumbnail.image.rakuten.co.jp/a.jpg"]
    );
    let b = &p.items[1];
    assert_eq!((b.price, b.rating), (3500, 3.5));
    assert!(!b.in_stock && !b.postage_included);

    let v1 = r#"{"count":1,"page":1,"pageCount":1,"items":[{"Item":{"itemName":"X","itemPrice":100,
        "mediumImageUrls":[{"imageUrl":"https://t/x.jpg"}]}}]}"#;
    let p = rakuten::parse_response(200, v1).unwrap();
    assert_eq!(p.items[0].name, "X");
    assert_eq!(p.items[0].images, vec!["https://t/x.jpg"]);
}

#[test]
fn rakuten_errors() {
    let e = rakuten::parse_response(
        403,
        r#"{"errors":{"errorCode":403,"errorMessage":"Invalid Access Key"}}"#,
    );
    assert!(e.unwrap_err().to_string().contains("Invalid Access Key"));
    let e = rakuten::parse_response(
        400,
        r#"{"error":"wrong_parameter","error_description":"specify valid applicationId"}"#,
    );
    assert!(
        e.unwrap_err()
            .to_string()
            .contains("specify valid applicationId")
    );
    let e = rakuten::parse_response(
        429,
        r#"{ "statusCode": 429, "message": "Rate limit is exceeded. Try again in 1 seconds." }"#,
    );
    assert!(e.unwrap_err().to_string().contains("Rate limit"));
}

#[test]
fn rakuten_url() {
    let creds = rakuten::Credentials {
        app_id: "APP".into(),
        access_key: "KEY".into(),
        referer: None,
    };
    let q = rakuten::Query {
        keyword: Some("イヤホン"),
        page: 2,
        hits: 50,
        sort: Some(rakuten::Sort::PriceAsc),
        min_price: Some(1000),
        in_stock: true,
        ..Default::default()
    };
    let url = rakuten::build_url(&creds, &q).unwrap();
    assert!(url.starts_with(rakuten::ENDPOINT));
    for part in [
        "applicationId=APP",
        "formatVersion=2",
        "page=2",
        "hits=30",
        "sort=%2BitemPrice",
        "minPrice=1000",
        "availability=1",
    ] {
        assert!(url.contains(part), "{url} lacks {part}");
    }
    assert!(
        !url.contains("KEY"),
        "access key must travel in a header, not the URL"
    );
}

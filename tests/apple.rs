use shop::{
    apple::{
        Catalog, Target, clean, config_url, explain_rejection, family_slug, models, parse_catalog,
        parse_config, parse_refurb, parse_target, plan, refurb_url,
    },
    fetch::FetchError,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/apple/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn mbp() -> Catalog {
    parse_catalog(&fixture("buy_macbook_pro.html"), "macbook-pro").unwrap()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn get<'a>(pairs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    pairs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
}

#[test]
fn catalog_from_buy_page() {
    let c = mbp();
    assert_eq!(c.name, "MacBook Pro");
    assert_eq!(c.url, "https://www.apple.com/jp/shop/buy-mac/macbook-pro");
    assert_eq!(
        c.config_url.as_deref(),
        Some("/jp/shop/api/cto/update-config?collection=MBP_2026_COLLECTION&fae=true")
    );
    assert_eq!(c.kits.len(), 7);
    let kit = c
        .kits
        .iter()
        .find(|k| k.key == "14inch-silver-standard-m5pro-15-16")
        .unwrap();
    assert_eq!(kit.price, Some(429800));
    assert_eq!(kit.part_number.as_deref(), Some("MGDN4J/A"));
    assert!(!kit.configurable);
    assert_eq!(
        c.kit_name(kit),
        "MacBook Pro 14インチ 標準ディスプレイ M5 Proチップ 15コアCPU、16コアGPU"
    );
    let memory = c.dim("memory-dimensionMemory").unwrap();
    assert!(!memory.main);
    assert_eq!(memory.name, "ユニファイドメモリ");
    let values: Vec<&str> = memory.values.iter().map(|v| v.value.as_str()).collect();
    assert_eq!(
        values,
        ["16gb", "24gb", "32gb", "36gb", "48gb", "64gb", "128gb"]
    );
    assert!(c.dim("chassis-dimensionScreensize").unwrap().main);
    assert_eq!(
        c.label(
            "processor-dimensionChip-cpuCoreCount-gpuCoreCount",
            "m6-12-12"
        ),
        "12コアCPU、12コアGPU",
        "falls back to the core counts when there is no header"
    );
    assert!(c.preselected.is_empty());
}

#[test]
fn models_merge_colors_and_sort_by_price() {
    let page = models(&mbp());
    assert_eq!(page.total, Some(5));
    let first = &page.items[0];
    assert_eq!(first.price, Some(339800));
    assert_eq!(
        first.name,
        "MacBook Pro 14インチ 標準ディスプレイ M6チップ 12コアCPU、12コアGPU"
    );
    assert_eq!(first.variants, ["シルバー", "スペースブラック"]);
    assert_eq!(
        get(&first.details, "model"),
        Some("macbook-pro/14inch-silver-standard-m6-12-12")
    );
    assert_eq!(get(&first.details, "part"), Some("MHWW4J/A, MHWT4J/A"));
    let prices: Vec<Option<u64>> = page.items.iter().map(|p| p.price).collect();
    assert_eq!(
        prices,
        [
            Some(339800),
            Some(429800),
            Some(456800),
            Some(459800),
            Some(519800)
        ]
    );
}

#[test]
fn model_page_carries_its_selection() {
    let c = parse_catalog(&fixture("model_m5pro_14.html"), "macbook-pro").unwrap();
    assert_eq!(c.name, "MacBook Pro");
    assert_eq!(get(&c.preselected, "memory-dimensionMemory"), Some("24gb"));
    let p = plan(&c, None, &[]).unwrap();
    assert_eq!(c.kits[p.kit].key, "14inch-silver-standard-m5pro-15-16");
    assert_eq!(get(&p.selection, "storage-dimensionCapacity"), Some("1tb"));
}

#[test]
fn plan_picks_the_cheapest_matching_model() {
    let c = mbp();
    let p = plan(&c, None, &strings(&["14inch", "M5Pro", "48gb"])).unwrap();
    let key = &c.kits[p.kit].key;
    assert!(
        key.starts_with("14inch-") && key.ends_with("-standard-m5pro-15-16"),
        "{key}"
    );
    assert_eq!(
        p.user_config,
        [("memory-dimensionMemory".to_string(), "48gb".to_string())]
    );
    assert_eq!(get(&p.selection, "memory-dimensionMemory"), Some("48gb"));

    let p = plan(&c, None, &[]).unwrap();
    assert_eq!(c.kits[p.kit].price, Some(339800));

    let p = plan(&c, None, &strings(&["m5pro-18-20", "16inch"])).unwrap();
    assert_eq!(c.kits[p.kit].key, "16inch-silver-standard-m5pro-18-20");
    assert_eq!(get(&p.selection, "processor-dimensionChip"), Some("m5pro"));

    let p = plan(
        &c,
        Some("14inch-spaceblack-standard-m6-12-12"),
        &strings(&["memory=32gb", "storage=2tb"]),
    )
    .unwrap();
    assert_eq!(c.kits[p.kit].key, "14inch-spaceblack-standard-m6-12-12");
    assert_eq!(get(&p.selection, "storage-dimensionCapacity"), Some("2tb"));

    let p = plan(
        &c,
        Some("14inch-spaceblack-standard-m6-12-12"),
        &strings(&["m5pro"]),
    )
    .unwrap();
    assert_eq!(
        c.kits[p.kit].key, "14inch-spaceblack-standard-m5pro-15-16",
        "keeps the model's other dimensions"
    );
}

#[test]
fn plan_rejects_bad_values() {
    let c = mbp();
    let err = |v: &[&str]| plan(&c, None, &strings(v)).unwrap_err().to_string();
    assert!(err(&["999gb"]).contains("no option 999gb"));
    assert!(err(&["none"]).contains("ambiguous"));
    assert!(err(&["16inch", "m6"]).contains("no MacBook Pro model"));
    assert!(err(&["memory=3gb"]).contains("16gb, 24gb"));
    let e = plan(&c, Some("nope"), &[]).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<FetchError>(),
        Some(FetchError::NotFound(_))
    ));
}

#[test]
fn configurator_url() {
    let c = mbp();
    let p = plan(&c, None, &strings(&["14inch", "m5pro", "48gb"])).unwrap();
    let url = config_url(&c, &p.selection).unwrap();
    assert!(url.starts_with(
        "https://www.apple.com/jp/shop/api/cto/update-config?collection=MBP_2026_COLLECTION&fae=true&"
    ));
    assert!(url.contains("&sv.chassis-dimensionScreensize=14inch"));
    assert!(url.contains("&sv.memory-dimensionMemory=48gb"));
    assert!(url.contains("&sections=chassis-dimensionScreensize%2C"));
}

#[test]
fn configuration_prices() {
    let c = mbp();
    let cfg = parse_config(&fixture("update_config_m5pro_48gb.json"), &c)
        .unwrap()
        .unwrap();
    assert_eq!(cfg.price, 537800);
    assert_eq!(cfg.model, "macbook-pro/14inch-silver-standard-m5pro-15-16");
    assert_eq!(cfg.part_number, None, "48GB is a custom build");
    assert_eq!(get(&cfg.selection, "memory-dimensionMemory"), Some("48gb"));
    assert_eq!(
        get(&cfg.selection, "storage-dimensionCapacity"),
        Some("1tb")
    );
    let memory = cfg
        .options
        .iter()
        .find(|g| g.key == "memory-dimensionMemory")
        .unwrap();
    assert_eq!(memory.name, "ユニファイドメモリ");
    let m24 = memory.choices.iter().find(|x| x.value == "24gb").unwrap();
    assert_eq!((m24.delta, m24.price), (Some(-108000), Some(429800)));
    let m48 = memory.choices.iter().find(|x| x.value == "48gb").unwrap();
    assert!(m48.selected && m48.available);
    let m64 = memory.choices.iter().find(|x| x.value == "64gb").unwrap();
    assert!(!m64.available);
    assert_eq!(m64.note.as_deref(), Some("needs another chip"));
    let storage = cfg
        .options
        .iter()
        .find(|g| g.key == "storage-dimensionCapacity")
        .unwrap();
    let s2 = storage.choices.iter().find(|x| x.value == "2tb").unwrap();
    assert_eq!((s2.delta, s2.price), (Some(90000), Some(627800)));
    assert_eq!(cfg.min_price, 429800);
    assert_eq!(
        cfg.max_price,
        537800 + 27000 + 270000 + 2000 + 30000 + 50000
    );
}

#[test]
fn rejected_configuration() {
    let c = mbp();
    let empty = r#"{"head":{"status":"200","data":{}},"body":{"isReset":false}}"#;
    assert!(parse_config(empty, &c).unwrap().is_none());
    assert!(parse_config("not json", &c).is_err());
    let base = fixture("update_config_m5pro_48gb.json");
    let why = explain_rejection(
        &base,
        &c,
        &[("memory-dimensionMemory".into(), "128gb".into())],
    );
    assert_eq!(why, "128gb needs another chip");
    let why = explain_rejection(
        &base,
        &c,
        &[("memory-dimensionMemory".into(), "16gb".into())],
    );
    assert_eq!(why, "ユニファイドメモリ can be 24gb, 48gb with this chip");
}

#[test]
fn refurb_listing() {
    let page = parse_refurb(&fixture("refurb_macbook_pro.html"), Some("macbook-pro")).unwrap();
    assert_eq!(
        page.total,
        Some(3),
        "the page filter keeps MacBook Pro tiles"
    );
    let p = &page.items[0];
    assert_eq!(p.id, "FDE14J/A");
    assert_eq!(p.price, Some(273800));
    assert!(p.name.contains("MacBook Pro [整備済製品]"));
    assert_eq!(p.url, "https://www.apple.com/jp/shop/product/fde14j/a");
    assert_eq!(p.categories, ["MacBook Pro"]);
    assert_eq!(get(&p.details, "サイズ"), Some("14インチ"));
    assert_eq!(get(&p.details, "メモリ"), Some("16GB"));
    assert_eq!(get(&p.details, "容量"), Some("1TB"));
    assert!(
        p.image
            .as_deref()
            .unwrap()
            .starts_with("https://store.storeimages")
    );

    let all = fixture("refurb_macbook_pro.html").replace(
        r#""selectedGridFilters": [{"refurbClearModel": "macbookpro"}]"#,
        r#""selectedGridFilters": null"#,
    );
    assert_eq!(parse_refurb(&all, None).unwrap().total, Some(9));
    assert_eq!(parse_refurb(&all, Some("imac")).unwrap().total, Some(1));
    assert!(parse_refurb("<html></html>", None).is_err());
}

#[test]
fn targets_and_urls() {
    assert_eq!(
        parse_target("macbook-pro").unwrap(),
        Target::Family("macbook-pro".into())
    );
    assert_eq!(
        parse_target("MacBook Pro").unwrap(),
        Target::Family("macbook-pro".into())
    );
    assert_eq!(
        parse_target("macbook-pro/14inch-silver-standard-m6-12-12").unwrap(),
        Target::Model {
            family: "macbook-pro".into(),
            key: "14inch-silver-standard-m6-12-12".into()
        }
    );
    assert_eq!(
        parse_target("https://www.apple.com/jp/shop/buy-mac/macbook-air").unwrap(),
        Target::Family("macbook-air".into())
    );
    let Target::Url { family, url } =
        parse_target("https://www.apple.com/jp/shop/buy-mac/macbook-pro/14インチ-silver-16gb?x=1")
            .unwrap()
    else {
        panic!("model URL")
    };
    assert_eq!(family, "macbook-pro");
    assert_eq!(
        url,
        "https://www.apple.com/jp/shop/buy-mac/macbook-pro/14%E3%82%A4%E3%83%B3%E3%83%81-silver-16gb"
    );
    assert!(parse_target("https://example.com/x").is_err());
    assert!(family_slug("ipad-pro").is_err());
    assert!(family_slug("").is_err());
    assert!(family_slug("mac;rm").is_err());
    assert_eq!(
        refurb_url(None).unwrap(),
        "https://www.apple.com/jp/shop/refurbished/mac"
    );
    assert_eq!(
        refurb_url(Some("iPad")).unwrap(),
        "https://www.apple.com/jp/shop/refurbished/ipad"
    );
    assert_eq!(
        refurb_url(Some("macbook-pro")).unwrap(),
        "https://www.apple.com/jp/shop/refurbished/mac/macbook-pro"
    );
    assert_eq!(
        refurb_url(Some("ipad-pro")).unwrap(),
        "https://www.apple.com/jp/shop/refurbished/ipad/ipad-pro"
    );
    assert_eq!(
        clean("<span>チップ。</span><span class=\"as-subheading\">選ぶ</span>"),
        "チップ"
    );
    assert_eq!(
        clean("14インチ<as-footnote>1</as-footnote> <div>x</div>"),
        "14インチ"
    );
}

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::Value;

use crate::{
    fetch::{FetchError, Fetcher},
    next::{self, text, yen},
    product::{Product, SearchPage},
};

pub const SITE: &str = "https://www.apple.com";
const SHOP: &str = "https://www.apple.com/jp/shop";
const CHIP: &str = "processor-dimensionChip";
const COLOR: &str = "chassis-dimensionColor";

#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Family(String),
    Model { family: String, key: String },
    Url { family: String, url: String },
}

#[derive(Debug)]
pub struct Catalog {
    pub family: String,
    pub name: String,
    pub url: String,
    pub config_url: Option<String>,
    pub kits: Vec<Kit>,
    pub dims: Vec<Dimension>,
    pub preselected: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Kit {
    pub key: String,
    pub part_number: Option<String>,
    pub configurable: bool,
    pub dims: Vec<(String, String)>,
    pub price: Option<u64>,
}

#[derive(Debug)]
pub struct Dimension {
    pub key: String,
    pub name: String,
    pub main: bool,
    pub values: Vec<DimValue>,
}

#[derive(Debug)]
pub struct DimValue {
    pub value: String,
    pub label: String,
    pub chip: Option<String>,
}

#[derive(Debug)]
pub struct Plan {
    pub kit: usize,
    pub selection: Vec<(String, String)>,
    pub user_config: Vec<(String, String)>,
}

#[derive(Debug, Serialize)]
pub struct Configuration {
    pub family: String,
    pub model: String,
    pub name: String,
    pub url: String,
    pub part_number: Option<String>,
    pub price: u64,
    pub min_price: u64,
    pub max_price: u64,
    pub selection: Vec<(String, String)>,
    pub options: Vec<OptionGroup>,
}

#[derive(Debug, Serialize)]
pub struct OptionGroup {
    pub key: String,
    pub name: String,
    pub note: Option<String>,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub selected: bool,
    pub available: bool,
    pub delta: Option<i64>,
    pub price: Option<u64>,
    pub note: Option<String>,
}

impl Catalog {
    pub fn dim(&self, key: &str) -> Option<&Dimension> {
        self.dims.iter().find(|d| d.key == key)
    }

    pub fn label(&self, key: &str, value: &str) -> String {
        self.dim(key)
            .and_then(|d| d.values.iter().find(|v| v.value == value))
            .map_or_else(|| value.to_string(), |v| v.label.clone())
    }

    pub fn model_id(&self, kit: &Kit) -> String {
        format!("{}/{}", self.family, kit.key)
    }

    pub fn kit_name(&self, kit: &Kit) -> String {
        let mut parts = vec![self.name.clone()];
        for d in self.dims.iter().filter(|d| d.main && d.key != COLOR) {
            if let Some((_, v)) = kit.dims.iter().find(|(k, _)| *k == d.key) {
                parts.push(self.label(&d.key, v));
            }
        }
        parts.join(" ")
    }
}

pub fn family_slug(input: &str) -> Result<String> {
    let f = input
        .trim()
        .trim_matches('/')
        .to_lowercase()
        .replace(' ', "-");
    if f.is_empty() {
        bail!("missing Apple family, e.g. macbook-pro");
    }
    if f.starts_with("ipad") || f.starts_with("iphone") {
        bail!(
            "Apple models and configure cover Mac families only (macbook-air, macbook-pro, macbook-neo, imac, mac-mini, mac-studio); iPad and iPhone pages use another layout"
        );
    }
    if !f
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("not an Apple family: {input}");
    }
    Ok(f)
}

pub fn buy_url(family: &str) -> String {
    format!("{SHOP}/buy-mac/{family}")
}

pub fn parse_target(input: &str) -> Result<Target> {
    let input = input.trim();
    if let Some(i) = input.find("/shop/buy-mac/") {
        let rest = input[i + "/shop/buy-mac/".len()..].split(['?', '#']).next();
        let mut segs = rest
            .unwrap_or_default()
            .split('/')
            .filter(|s| !s.is_empty());
        let family = family_slug(segs.next().unwrap_or_default())?;
        return Ok(match segs.next() {
            Some(slug) => {
                let url = url::Url::parse(&format!("{}/{slug}", buy_url(&family)))?;
                Target::Url {
                    family,
                    url: url.into(),
                }
            }
            None => Target::Family(family),
        });
    }
    if input.contains("://") {
        bail!("not an Apple Store model URL: {input}");
    }
    match input.split_once('/') {
        Some((family, key)) if !key.trim().is_empty() => Ok(Target::Model {
            family: family_slug(family)?,
            key: key.trim().to_string(),
        }),
        _ => Ok(Target::Family(family_slug(input)?)),
    }
}

fn json_after(html: &str, marker: &str) -> Option<Value> {
    use serde::Deserialize;
    let i = html.find(marker)? + marker.len();
    let mut de = serde_json::Deserializer::from_str(html[i..].trim_start());
    Value::deserialize(&mut de).ok()
}

fn quoted_after(html: &str, marker: &str) -> Option<String> {
    let i = html.find(marker)? + marker.len();
    let rest = html[i..].trim_start();
    let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let end = rest[1..].find(q)?;
    text(&Value::String(rest[1..1 + end].to_string()))
}

pub fn clean(html: &str) -> String {
    let mut s = html;
    for cut in ["<div", "<as-footnote", "<span class=\"as-subheading"] {
        if let Some(i) = s.find(cut) {
            s = &s[..i];
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&nbsp;", " ").replace('\u{a0}', " ");
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('。')
        .to_string()
}

fn values_of(dv: &Value) -> Vec<DimValue> {
    let Some(map) = dv.as_object() else {
        return Vec::new();
    };
    let order: Vec<String> = match dv["variantOrder"].as_array() {
        Some(o) => o.iter().filter_map(text).collect(),
        None => map
            .keys()
            .filter(|k| *k != "variantOrder")
            .cloned()
            .collect(),
    };
    order
        .into_iter()
        .map(|value| {
            let x = &dv[&value];
            let comp = &x["dimensionComponents"];
            let label = text(&x["header"])
                .map(|h| clean(&h))
                .filter(|l| !l.is_empty())
                .or_else(|| {
                    let (c, g) = (text(&comp["cpuCoreCount"])?, text(&comp["gpuCoreCount"])?);
                    Some(format!("{c}コアCPU、{g}コアGPU"))
                })
                .unwrap_or_else(|| value.clone());
            DimValue {
                chip: text(&comp["dimensionChip"]),
                label,
                value,
            }
        })
        .collect()
}

fn section_names(sections: &Value, out: &mut Vec<(String, String)>) {
    for s in sections.as_array().into_iter().flatten() {
        if let Some(k) = text(&s["formFieldName"]) {
            out.push((k, text(&s["header"]).map(|h| clean(&h)).unwrap_or_default()));
        }
        section_names(&s["items"], out);
    }
}

pub fn parse_catalog(html: &str, family: &str) -> Result<Catalog> {
    let Some(data) = json_after(html, "productSelectionData:") else {
        bail!("Apple: no product data on the {family} page (layout changed or not a Mac family?)");
    };
    let main = &data["mainDisplayValues"];
    let config = &data["configDisplayValues"];
    let prices = if data["prices"].is_object() {
        &data["prices"]
    } else {
        &main["prices"]
    };
    let mut names = Vec::new();
    section_names(&data["mainSections"], &mut names);
    section_names(&data["configSections"], &mut names);
    let mut keys: Vec<String> = names.iter().map(|(k, _)| k.clone()).collect();
    for m in [main, config] {
        keys.extend(m.as_object().into_iter().flat_map(|o| o.keys().cloned()));
    }
    let mut dims: Vec<Dimension> = Vec::new();
    for key in keys {
        if key == "prices" || dims.iter().any(|d| d.key == key) {
            continue;
        }
        let (dv, is_main) = if main[&key].is_object() {
            (&main[&key], true)
        } else if config[&key].is_object() {
            (&config[&key], false)
        } else {
            continue;
        };
        let name = names
            .iter()
            .find(|(k, n)| *k == key && !n.is_empty())
            .map_or_else(|| key.clone(), |(_, n)| n.clone());
        dims.push(Dimension {
            values: values_of(dv),
            main: is_main,
            name,
            key,
        });
    }
    let kits: Vec<Kit> = data["products"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let key = text(&p["priceKey"])?;
            let dims = p["dimensions"]
                .as_object()?
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), text(v)?)))
                .collect();
            Some(Kit {
                price: yen(&prices[&key]["amount"])
                    .or_else(|| yen(&prices[&key]["currentPrice"]["raw_amount"])),
                part_number: text(&p["btrOrFdPartNumber"]),
                configurable: p["type"].as_str() == Some("CONFIGURABLE"),
                dims,
                key,
            })
        })
        .collect();
    if kits.is_empty() {
        bail!("Apple: no models on the {family} page (layout changed?)");
    }
    let preselected = json_after(html, "warmStateBootstrap = ")
        .and_then(|w| {
            w["preselectedDimensions"].as_object().map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.clone(), text(v)?)))
                    .collect()
            })
        })
        .unwrap_or_default();
    let lds = next::ld_json(html);
    let name = next::ld_of_type(&lds, "Product")
        .and_then(|p| text(&p["name"]))
        .map(|n| {
            n.split('、')
                .next()
                .unwrap_or_default()
                .replace('\u{a0}', " ")
        })
        .unwrap_or_else(|| family.to_string());
    Ok(Catalog {
        family: family.to_string(),
        url: buy_url(family),
        config_url: quoted_after(html, "updateConfigUrl:"),
        name,
        kits,
        dims,
        preselected,
    })
}

pub fn models(catalog: &Catalog) -> SearchPage {
    let mut groups: Vec<(&Kit, Vec<&Kit>)> = Vec::new();
    for kit in &catalog.kits {
        let same = |a: &Kit| {
            a.price == kit.price
                && a.dims.len() == kit.dims.len()
                && a.dims
                    .iter()
                    .all(|(k, v)| k == COLOR || kit.dims.iter().any(|(k2, v2)| k2 == k && v2 == v))
        };
        match groups.iter_mut().find(|(first, _)| same(first)) {
            Some((_, members)) => members.push(kit),
            None => groups.push((kit, vec![kit])),
        }
    }
    groups.sort_by_key(|(k, _)| (k.price.unwrap_or(u64::MAX), k.key.clone()));
    let items: Vec<Product> = groups
        .into_iter()
        .map(|(first, members)| {
            let mut details = vec![("model".to_string(), catalog.model_id(first))];
            let colors: Vec<String> = members
                .iter()
                .filter_map(|m| m.dims.iter().find(|(k, _)| k == COLOR))
                .map(|(k, v)| catalog.label(k, v))
                .collect();
            if !colors.is_empty() {
                details.push(("colors".to_string(), colors.join(", ")));
            }
            let parts: Vec<String> = members
                .iter()
                .filter_map(|m| m.part_number.clone())
                .collect();
            if !parts.is_empty() {
                details.push(("part".to_string(), parts.join(", ")));
            }
            Product {
                id: catalog.model_id(first),
                name: catalog.kit_name(first),
                url: catalog.url.clone(),
                price: first.price,
                variants: colors,
                details,
                ..Product::default()
            }
        })
        .collect();
    SearchPage {
        total: Some(items.len() as u64),
        page: 1,
        items,
    }
}

fn find_dim<'a>(catalog: &'a Catalog, key: &str) -> Option<&'a Dimension> {
    let key = key.to_lowercase();
    catalog.dim(&key).or_else(|| {
        catalog.dims.iter().find(|d| {
            d.key
                .to_lowercase()
                .split('-')
                .next()
                .is_some_and(|p| p == key)
        })
    })
}

fn choices_text(d: &Dimension) -> String {
    d.values
        .iter()
        .map(|v| v.value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn plan(catalog: &Catalog, model: Option<&str>, values: &[String]) -> Result<Plan> {
    let mut picks: Vec<(String, String)> = Vec::new();
    for raw in values {
        let raw = raw.trim();
        let (dim, value) = match raw.split_once('=') {
            Some((k, v)) => {
                let d = find_dim(catalog, k.trim())
                    .ok_or_else(|| anyhow!("{} has no option {k}", catalog.name))?;
                let v = v.trim().to_lowercase();
                if !d.values.iter().any(|x| x.value == v) {
                    bail!("{} {} can be: {}", catalog.name, d.key, choices_text(d));
                }
                (d, v)
            }
            None => {
                let v = raw.to_lowercase();
                let hits: Vec<&Dimension> = catalog
                    .dims
                    .iter()
                    .filter(|d| d.values.iter().any(|x| x.value == v))
                    .collect();
                match hits.as_slice() {
                    [d] => (*d, v),
                    [] => bail!(
                        "{} has no option {raw} (run `shop apple configure {}` to list them)",
                        catalog.name,
                        catalog.family
                    ),
                    many => bail!(
                        "{raw} is ambiguous ({}); use dimension=value",
                        many.iter()
                            .map(|d| d.key.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            }
        };
        if let Some(chip) = dim
            .values
            .iter()
            .find(|x| x.value == value)
            .and_then(|x| x.chip.clone())
            && dim.key != CHIP
            && catalog.dim(CHIP).is_some()
        {
            picks.retain(|(k, _)| k != CHIP);
            picks.push((CHIP.to_string(), chip));
        }
        picks.retain(|(k, _)| *k != dim.key);
        picks.push((dim.key.clone(), value));
    }
    let is_main = |k: &str| catalog.dim(k).is_some_and(|d| d.main);
    let base: Vec<(String, String)> = match model {
        Some(key) => catalog
            .kits
            .iter()
            .find(|k| k.key == key)
            .ok_or_else(|| FetchError::NotFound(format!("{} model {key}", catalog.name)))?
            .dims
            .clone(),
        None => catalog.preselected.clone(),
    };
    let wanted: Vec<&(String, String)> = picks.iter().filter(|(k, _)| is_main(k)).collect();
    let matches =
        |kit: &Kit, (k, v): &(String, String)| kit.dims.iter().any(|(a, b)| a == k && b == v);
    let kit = catalog
        .kits
        .iter()
        .enumerate()
        .filter(|(_, kit)| wanted.iter().all(|w| matches(kit, w)))
        .max_by_key(|(i, kit)| {
            let score = base
                .iter()
                .filter(|b| is_main(&b.0) && matches(kit, b))
                .count();
            (
                score,
                std::cmp::Reverse(kit.price.unwrap_or(u64::MAX)),
                std::cmp::Reverse(*i),
            )
        })
        .map(|(i, _)| i)
        .ok_or_else(|| {
            let sel: Vec<&str> = wanted.iter().map(|(_, v)| v.as_str()).collect();
            anyhow!("no {} model with {}", catalog.name, sel.join(" "))
        })?;
    let mut selection = catalog.kits[kit].dims.clone();
    for (k, v) in &base {
        if !is_main(k) && !selection.iter().any(|(a, _)| a == k) {
            selection.push((k.clone(), v.clone()));
        }
    }
    let user_config: Vec<(String, String)> =
        picks.into_iter().filter(|(k, _)| !is_main(k)).collect();
    for (k, v) in &user_config {
        selection.retain(|(a, _)| a != k);
        selection.push((k.clone(), v.clone()));
    }
    Ok(Plan {
        kit,
        selection,
        user_config,
    })
}

pub fn config_url(catalog: &Catalog, selection: &[(String, String)]) -> Result<String> {
    let Some(path) = &catalog.config_url else {
        bail!(
            "Apple: the {} page has no configurator endpoint",
            catalog.name
        );
    };
    let mut url = url::Url::parse(&format!("{SITE}{path}"))?;
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in selection {
            q.append_pair(&format!("sv.{k}"), v);
        }
        let sections: Vec<&str> = catalog.dims.iter().map(|d| d.key.as_str()).collect();
        q.append_pair("sections", &sections.join(","));
    }
    Ok(url.into())
}

fn delta(prices: &Value, key: &str) -> Option<i64> {
    prices[key]["amount"]
        .as_f64()
        .map(|a| a.round() as i64)
        .or_else(|| key.split('_').next()?.parse().ok())
}

pub fn parse_config(body: &str, catalog: &Catalog) -> Result<Option<Configuration>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| anyhow!("Apple configurator returned invalid JSON: {e}"))?;
    let b = &v["body"];
    let kit = &b["selectedKits"];
    let Some(price) = yen(&kit["priceData"]["amount"])
        .or_else(|| yen(&kit["priceData"]["currentPrice"]["raw_amount"]))
    else {
        return Ok(None);
    };
    let selected: Vec<(String, String)> = kit["dimensions"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, d)| Some((k.clone(), text(&d["dimensionValue"])?)))
        .collect();
    let options = &b["options"];
    let mut groups = Vec::new();
    let (mut lo, mut hi) = (0i64, 0i64);
    for d in &catalog.dims {
        let o = &options[&d.key];
        if !o.is_object() {
            continue;
        }
        let compat = &o["compatibleOptions"];
        let upgrade = &o["upgradeOptions"];
        let current = selected
            .iter()
            .find(|(k, _)| *k == d.key)
            .map(|(_, v)| v.as_str());
        let mut order: Vec<String> = d.values.iter().map(|x| x.value.clone()).collect();
        for m in [compat, upgrade] {
            for k in m.as_object().into_iter().flat_map(|o| o.keys()) {
                if !order.contains(k) {
                    order.push(k.clone());
                }
            }
        }
        let choices: Vec<Choice> = order
            .into_iter()
            .filter_map(|value| {
                let c = &compat[&value];
                let up = &upgrade[&value];
                if !c.is_object() && !up.is_object() {
                    return None;
                }
                let blocked = c["isBlocked"].as_bool() == Some(true);
                let delta = c["priceDelta"]
                    .as_str()
                    .and_then(|k| delta(&b["prices"], k));
                let note = if !c.is_object() {
                    Some("needs another chip".to_string())
                } else if blocked {
                    Some("not available with this selection".to_string())
                } else {
                    None
                };
                Some(Choice {
                    label: catalog.label(&d.key, &value),
                    selected: current == Some(value.as_str()),
                    available: c.is_object() && !blocked,
                    price: delta.and_then(|x| u64::try_from(price as i64 + x).ok()),
                    delta,
                    note,
                    value,
                })
            })
            .collect();
        if !d.key.starts_with("processor") {
            let ds: Vec<i64> = choices
                .iter()
                .filter(|c| c.available)
                .filter_map(|c| c.delta)
                .collect();
            lo += ds.iter().min().copied().unwrap_or(0);
            hi += ds.iter().max().copied().unwrap_or(0);
        }
        groups.push(OptionGroup {
            key: d.key.clone(),
            name: d.name.clone(),
            note: text(&o["dynamicFooter"]),
            choices,
        });
    }
    let model = catalog.kits.iter().find(|kit| {
        kit.dims
            .iter()
            .all(|(k, v)| selected.iter().any(|(a, b)| a == k && b == v))
    });
    let name = match model {
        Some(k) => catalog.kit_name(k),
        None => catalog.name.clone(),
    };
    let mut ordered = Vec::new();
    for d in &catalog.dims {
        if let Some((k, v)) = selected.iter().find(|(k, _)| *k == d.key) {
            ordered.push((k.clone(), v.clone()));
        }
    }
    Ok(Some(Configuration {
        family: catalog.family.clone(),
        model: model.map_or_else(|| catalog.family.clone(), |k| catalog.model_id(k)),
        name,
        url: catalog.url.clone(),
        part_number: text(&kit["btrOrFdPartNumber"]),
        price,
        min_price: u64::try_from(price as i64 + lo).unwrap_or(0),
        max_price: u64::try_from(price as i64 + hi).unwrap_or(price),
        selection: ordered,
        options: groups,
    }))
}

pub fn explain_rejection(base: &str, catalog: &Catalog, user: &[(String, String)]) -> String {
    let v: Value = serde_json::from_str(base).unwrap_or_default();
    let options = &v["body"]["options"];
    let mut why = Vec::new();
    for (k, val) in user {
        let o = &options[k];
        let compat: Vec<String> = o["compatibleOptions"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys().cloned())
            .collect();
        if compat.contains(val) {
            continue;
        }
        let what = catalog.dim(k).map_or(k.as_str(), |d| d.name.as_str());
        if o["upgradeOptions"][val].is_object() {
            why.push(format!("{val} needs another chip"));
        } else if !compat.is_empty() {
            why.push(format!(
                "{what} can be {} with this chip",
                compat.join(", ")
            ));
        }
    }
    if why.is_empty() {
        "Apple has no configuration for this selection".to_string()
    } else {
        why.join("; ")
    }
}

pub fn refurb_url(family: Option<&str>) -> Result<String> {
    let f = family
        .map(|f| f.trim().trim_matches('/').to_lowercase().replace(' ', "-"))
        .filter(|f| !f.is_empty());
    let path = match f.as_deref() {
        None | Some("mac") => "mac".to_string(),
        Some(f @ ("ipad" | "iphone")) => f.to_string(),
        Some(f) if !f.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            bail!("not an Apple family: {f}")
        }
        Some(f) if f.starts_with("ipad") => format!("ipad/{f}"),
        Some(f) if f.starts_with("iphone") => format!("iphone/{f}"),
        Some(f) => format!("mac/{f}"),
    };
    Ok(format!("{SHOP}/refurbished/{path}"))
}

pub fn parse_refurb(html: &str, family: Option<&str>) -> Result<SearchPage> {
    let Some(data) = json_after(html, "REFURB_GRID_BOOTSTRAP = ") else {
        bail!("Apple: no refurbished listings on the page (layout changed?)");
    };
    let filters: Vec<(String, String)> = data["selectedGridFilters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f.as_object())
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), text(v)?)))
        .collect();
    let prefix = family
        .map(|f| f.trim().to_lowercase().replace(['-', ' '], ""))
        .filter(|f| filters.is_empty() && !matches!(f.as_str(), "" | "mac" | "ipad" | "iphone"));
    let dict = &data["dictionaries"]["dimensions"];
    let legends: Vec<(String, String)> = data["dimensions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| Some((text(&d["key"])?, text(&d["legend"])?)))
        .collect();
    let items: Vec<Product> = data["tiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| {
            filters
                .iter()
                .all(|(k, v)| t["filters"]["dimensions"][k].as_str() == Some(v))
                && prefix.as_ref().is_none_or(|p| {
                    t["filters"]["dimensions"]["refurbClearModel"]
                        .as_str()
                        .is_some_and(|m| m.starts_with(p.as_str()))
                })
        })
        .filter_map(|t| {
            let dims = &t["filters"]["dimensions"];
            let name = |k: &str| {
                let v = text(&dims[k])?;
                Some(text(&dict[k][&v]["text"]).unwrap_or(v))
            };
            let details = legends
                .iter()
                .filter(|(k, _)| k != "refurbClearModel")
                .filter_map(|(k, legend)| Some((legend.clone(), name(k)?)))
                .collect();
            let path = text(&t["productDetailsUrl"])?;
            let path: Vec<&str> = path.split(['?', '#']).next()?.split('/').take(6).collect();
            let path = path.join("/");
            Some(Product {
                id: text(&t["partNumber"]).or_else(|| text(&t["price"]["partNumber"]))?,
                name: text(&t["title"])?,
                url: format!("{SITE}{path}"),
                price: yen(&t["price"]["currentPrice"]["raw_amount"]),
                categories: name("refurbClearModel").into_iter().collect(),
                details,
                image: t["image"]["sources"]
                    .as_array()
                    .and_then(|s| s.last())
                    .and_then(|s| text(&s["srcSet"])),
                ..Product::default()
            })
        })
        .collect();
    Ok(SearchPage {
        total: Some(items.len() as u64),
        page: 1,
        items,
    })
}

fn fetch(fetcher: &Fetcher, url: &str, accept: &str) -> Result<String> {
    let r = fetcher.get(
        url,
        &[("Accept", accept), ("Accept-Language", "ja,en;q=0.8")],
    )?;
    match r.status {
        404 => Err(FetchError::NotFound(format!("Apple Store page {url}")).into()),
        403 | 429 | 503 => Err(FetchError::Blocked(format!("Apple: HTTP {}", r.status)).into()),
        s if s >= 400 => bail!("Apple returned HTTP {s} for {url}"),
        _ => Ok(r.body),
    }
}

const HTML: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

pub fn fetch_catalog(fetcher: &Fetcher, family: &str) -> Result<Catalog> {
    parse_catalog(&fetch(fetcher, &buy_url(family), HTML)?, family)
}

pub fn fetch_refurb(fetcher: &Fetcher, family: Option<&str>) -> Result<SearchPage> {
    parse_refurb(&fetch(fetcher, &refurb_url(family)?, HTML)?, family)
}

pub fn configure(fetcher: &Fetcher, target: &str, values: &[String]) -> Result<Configuration> {
    let (catalog, model) = match parse_target(target)? {
        Target::Family(f) => (fetch_catalog(fetcher, &f)?, None),
        Target::Model { family, key } => (fetch_catalog(fetcher, &family)?, Some(key)),
        Target::Url { family, url } => {
            (parse_catalog(&fetch(fetcher, &url, HTML)?, &family)?, None)
        }
    };
    let plan = plan(&catalog, model.as_deref(), values)?;
    let json = "application/json";
    let body = fetch(fetcher, &config_url(&catalog, &plan.selection)?, json)?;
    if let Some(c) = parse_config(&body, &catalog)? {
        return Ok(c);
    }
    if plan.user_config.is_empty() {
        bail!(
            "Apple has no configuration for {}",
            catalog.model_id(&catalog.kits[plan.kit])
        );
    }
    let base: Vec<(String, String)> = plan
        .selection
        .iter()
        .filter(|s| !plan.user_config.contains(s))
        .cloned()
        .collect();
    let body = fetch(fetcher, &config_url(&catalog, &base)?, json)?;
    bail!(
        "{}: {}",
        catalog.kit_name(&catalog.kits[plan.kit]),
        explain_rejection(&body, &catalog, &plan.user_config)
    )
}

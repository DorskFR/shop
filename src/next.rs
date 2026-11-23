use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;

/// Joins the React Server Components payload that Next.js App Router pages stream
/// through `self.__next_f.push([1, "..."])` scripts.
pub fn flight(html: &str) -> String {
    const MARK: &str = "self.__next_f.push([1,";
    let mut out = String::new();
    let mut rest = html;
    while let Some(i) = rest.find(MARK) {
        rest = rest[i + MARK.len()..].trim_start();
        let mut de = serde_json::Deserializer::from_str(rest);
        if let Ok(chunk) = String::deserialize(&mut de) {
            out.push_str(&chunk);
        }
    }
    out
}

/// Every JSON value that follows `"key":` in the payload, in document order.
pub fn values<'a>(text: &'a str, key: &str) -> impl Iterator<Item = Value> + 'a {
    let needle = format!("\"{key}\":");
    let mut pos = 0;
    std::iter::from_fn(move || {
        loop {
            let i = pos + text.get(pos..)?.find(&needle)?;
            pos = i + needle.len();
            let mut de = serde_json::Deserializer::from_str(&text[pos..]);
            if let Ok(v) = Value::deserialize(&mut de) {
                return Some(v);
            }
        }
    })
}

pub fn ld_json(html: &str) -> Vec<Value> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("script[type=\"application/ld+json\"]").expect("static selector");
    doc.select(&sel)
        .filter_map(|s| serde_json::from_str(&s.text().collect::<String>()).ok())
        .collect()
}

pub fn ld_of_type<'a>(lds: &'a [Value], kind: &str) -> Option<&'a Value> {
    lds.iter().find(|v| v["@type"].as_str() == Some(kind))
}

pub fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())
}

pub fn yen(v: &Value) -> Option<u64> {
    num(v).filter(|n| *n >= 0.0).map(|n| n.round() as u64)
}

pub fn rating(score: &Value, reviews: Option<u64>) -> Option<f64> {
    let r = num(score)?;
    (reviews.unwrap_or(0) > 0 && r > 0.0).then(|| (r * 100.0).round() / 100.0)
}

pub fn push_unique(list: &mut Vec<String>, s: String) {
    if !s.is_empty() && !list.contains(&s) {
        list.push(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_chunks_and_finds_values() {
        let html = r#"<script>self.__next_f.push([1,"0:{\"a\":{\"b\""])</script>
<script>self.__next_f.push([1, ":[1,2]},\"a\":7}\n"])</script>"#;
        let f = flight(html);
        assert_eq!(f, "0:{\"a\":{\"b\":[1,2]},\"a\":7}\n");
        let vals: Vec<Value> = values(&f, "a").collect();
        assert_eq!(
            vals,
            [serde_json::json!({"b": [1, 2]}), serde_json::json!(7)]
        );
        assert_eq!(values(&f, "zz").count(), 0);
    }

    #[test]
    fn numbers_from_strings() {
        assert_eq!(num(&serde_json::json!("4.73")), Some(4.73));
        assert_eq!(rating(&serde_json::json!(4.6), Some(0)), None);
        assert_eq!(rating(&serde_json::json!(4.567), Some(3)), Some(4.57));
    }
}

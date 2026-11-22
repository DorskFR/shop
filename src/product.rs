use anyhow::Result;
use serde::Serialize;

use crate::fetch::Fetcher;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sort {
    Relevance,
    PriceAsc,
    PriceDesc,
    Reviews,
    Newest,
}

pub struct Query<'a> {
    pub text: &'a str,
    pub page: u32,
    pub limit: Option<u32>,
    pub sort: Sort,
}

#[derive(Debug, Default, Serialize)]
pub struct SearchPage {
    pub total: Option<u64>,
    pub page: u32,
    pub items: Vec<Product>,
}

#[derive(Debug, Default, Serialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub url: String,
    pub brand: Option<String>,
    pub price: Option<u64>,
    pub list_price: Option<u64>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,
    pub in_stock: Option<bool>,
    pub badges: Vec<String>,
    pub variants: Vec<String>,
    pub categories: Vec<String>,
    pub details: Vec<(String, String)>,
    pub description: Option<String>,
    pub image: Option<String>,
}

pub trait Connector {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage>;
    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Product>;
}

pub fn unsupported_sort(shop: &str, sort: Sort) -> anyhow::Error {
    anyhow::anyhow!("{shop} search does not support --sort {sort:?}")
}

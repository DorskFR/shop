use anyhow::Result;
use serde::Serialize;

use crate::fetch::Fetcher;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sort {
    Relevance,
    Rating,
    Reviews,
    Newest,
}

pub struct Query<'a> {
    pub text: &'a str,
    pub area: Option<&'a str>,
    pub page: u32,
    pub limit: Option<u32>,
    pub sort: Sort,
}

#[derive(Debug, Default, Serialize)]
pub struct SearchPage {
    pub total: Option<u64>,
    pub page: u32,
    pub items: Vec<Venue>,
}

/// A place you visit: a salon, a restaurant.
#[derive(Debug, Default, Serialize)]
pub struct Venue {
    pub id: String,
    pub name: String,
    pub url: String,
    pub genre: Vec<String>,
    pub area: Option<String>,
    pub address: Option<String>,
    pub access: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<u64>,
    pub budget: Option<String>,
    pub hours: Option<String>,
    pub closed: Option<String>,
    pub phone: Option<String>,
    pub booking_url: Option<String>,
    pub badges: Vec<String>,
    pub details: Vec<(String, String)>,
    pub menus: Vec<Menu>,
    pub description: Option<String>,
    pub image: Option<String>,
}

/// A coupon, course or menu entry.
#[derive(Debug, Default, Serialize)]
pub struct Menu {
    pub name: String,
    pub price: Option<u64>,
    pub price_text: Option<String>,
    pub description: Option<String>,
}

pub trait Connector {
    fn search(&self, fetcher: &Fetcher, query: &Query) -> Result<SearchPage>;
    fn item(&self, fetcher: &Fetcher, id_or_url: &str) -> Result<Venue>;
}

pub fn unsupported_sort(site: &str, sort: Sort) -> anyhow::Error {
    anyhow::anyhow!("{site} search does not support --sort {sort:?}")
}

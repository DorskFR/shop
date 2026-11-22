use anyhow::{Result, bail};

use crate::{
    fetch::Fetcher,
    product::{Connector, Product, Query, SearchPage},
};

pub struct Ikea;

impl Connector for Ikea {
    fn search(&self, _fetcher: &Fetcher, _query: &Query) -> Result<SearchPage> {
        bail!("not implemented")
    }

    fn item(&self, _fetcher: &Fetcher, _id_or_url: &str) -> Result<Product> {
        bail!("not implemented")
    }
}

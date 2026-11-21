pub mod level;
pub mod mock;

use std::time::SystemTime;

use async_trait::async_trait;

#[async_trait]
pub trait UsageProvider: Send {
    fn id(&self) -> &'static str;
    async fn fetch(&mut self) -> anyhow::Result<Usage>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Usage {
    pub plan: Option<String>,
    pub meters: Vec<Meter>,
    pub spend: Option<Spend>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    pub label: String,
    pub used: f64,
    pub limit: f64,
    pub reset_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spend {
    pub used: f64,
    pub limit: f64,
    pub currency: String,
}

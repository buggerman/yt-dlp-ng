use crate::core::VideoMetadata;
use anyhow::Result;
use async_trait::async_trait;
use url::Url;

#[async_trait]
pub trait Extractor: Send + Sync {
    fn name(&self) -> &'static str;
    fn suitable(&self, url: &Url) -> bool;
    async fn extract(&mut self, url: &Url) -> Result<VideoMetadata>;
}

pub struct ExtractorEngine {
    pub extractors: Vec<Box<dyn Extractor>>,
}

impl ExtractorEngine {
    pub fn new() -> Self {
        Self {
            extractors: vec![
                // TODO: Add built-in extractors
            ],
        }
    }

    pub fn register_extractor(&mut self, extractor: Box<dyn Extractor>) {
        self.extractors.push(extractor);
    }

    pub async fn extract(&mut self, url: &str) -> Result<VideoMetadata> {
        let parsed_url = Url::parse(url)?;

        for extractor in &mut self.extractors {
            if extractor.suitable(&parsed_url) {
                return extractor.extract(&parsed_url).await;
            }
        }

        anyhow::bail!("No suitable extractor found for URL: {}", url);
    }
}

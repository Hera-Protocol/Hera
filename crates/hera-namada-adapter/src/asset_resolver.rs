use std::collections::HashMap;

use hera_types::Asset;
use serde::Deserialize;

use crate::error::NamadaAdapterError;

/// Resolves native and IBC asset metadata because Namada is fundamentally
/// multi-asset and the compliance layer must preserve exact asset identity.
pub struct AssetResolver {
    pub node_url: String,
    pub cache: HashMap<String, Asset>,
}

impl AssetResolver {
    pub fn new(node_url: impl Into<String>) -> Self {
        Self {
            node_url: node_url.into(),
            cache: HashMap::new(),
        }
    }

    /// Resolves an asset identifier into stable metadata. IBC asset symbols can
    /// change across chain upgrades. Always resolve from chain state — do not
    /// hardcode non-native assets.
    pub async fn resolve(&mut self, asset_id: &str) -> Result<Asset, NamadaAdapterError> {
        if let Some(asset) = self.cache.get(asset_id) {
            return Ok(asset.clone());
        }

        if asset_id.eq_ignore_ascii_case("nam") || asset_id.eq_ignore_ascii_case("native:nam") {
            let asset = Asset {
                symbol: "NAM".to_string(),
                asset_id: asset_id.to_string(),
                decimals: 6,
            };
            self.cache.insert(asset_id.to_string(), asset.clone());
            return Ok(asset);
        }

        let base = self.node_url.trim_end_matches('/');
        let response = reqwest::get(format!("{base}/api/v1/assets/{asset_id}"))
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::AssetResolutionFailed(err.to_string()))?
            .json::<AssetMetadataResponse>()
            .await
            .map_err(|err| NamadaAdapterError::AssetResolutionFailed(err.to_string()))?;

        let asset = Asset {
            symbol: response.symbol,
            asset_id: asset_id.to_string(),
            decimals: response.decimals,
        };
        self.cache.insert(asset_id.to_string(), asset.clone());
        Ok(asset)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct AssetMetadataResponse {
    symbol: String,
    decimals: u8,
}

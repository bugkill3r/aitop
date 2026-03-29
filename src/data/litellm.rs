use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::pricing::ModelPrice;

const LITELLM_URL: &str = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

/// Per-model entry from LiteLLM pricing JSON.
#[derive(Debug, Deserialize)]
struct LiteLLMEntry {
    #[serde(default)]
    input_cost_per_token: Option<f64>,
    #[serde(default)]
    output_cost_per_token: Option<f64>,
    #[serde(default)]
    cache_read_input_token_cost: Option<f64>,
    #[serde(default)]
    cache_creation_input_token_cost: Option<f64>,
}

/// Fetch LiteLLM pricing JSON and save to cache file.
pub fn fetch_and_cache(cache_path: &Path) -> Result<()> {
    eprintln!("  Fetching pricing data from LiteLLM...");
    let body = ureq::get(LITELLM_URL).call()?.into_body().read_to_string()?;

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(cache_path, &body)?;
    eprintln!("  Pricing cache updated: {}", cache_path.display());
    Ok(())
}

/// Load cached LiteLLM pricing if the cache exists and is less than 24h old.
pub fn load_cached(cache_path: &Path) -> Result<HashMap<String, ModelPrice>> {
    if !cache_path.exists() {
        return Ok(HashMap::new());
    }

    // Check age
    let metadata = std::fs::metadata(cache_path)?;
    let age = metadata
        .modified()?
        .elapsed()
        .unwrap_or_default();

    if age.as_secs() > 86400 {
        return Ok(HashMap::new()); // Stale cache
    }

    let content = std::fs::read_to_string(cache_path)?;
    parse_litellm_json(&content)
}

/// Parse the LiteLLM JSON into a map of model name → ModelPrice.
fn parse_litellm_json(json: &str) -> Result<HashMap<String, ModelPrice>> {
    let entries: HashMap<String, LiteLLMEntry> = serde_json::from_str(json)?;
    let mut prices = HashMap::new();

    for (model, entry) in entries {
        // Skip entries without pricing data
        let input_per_token = match entry.input_cost_per_token {
            Some(v) if v > 0.0 => v,
            _ => continue,
        };
        let output_per_token = match entry.output_cost_per_token {
            Some(v) if v > 0.0 => v,
            _ => continue,
        };

        // Convert per-token to per-million-token
        let input = input_per_token * 1_000_000.0;
        let output = output_per_token * 1_000_000.0;
        let cache_read = entry
            .cache_read_input_token_cost
            .map(|v| v * 1_000_000.0)
            .unwrap_or(input * 0.1); // Default: 10% of input
        let cache_creation = entry
            .cache_creation_input_token_cost
            .map(|v| v * 1_000_000.0)
            .unwrap_or(input * 2.0); // Default: 2x input

        prices.insert(model, ModelPrice {
            input,
            output,
            cache_read,
            cache_creation,
        });
    }

    Ok(prices)
}

/// Returns the default cache file path.
pub fn cache_path() -> std::path::PathBuf {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join(".local").join("share"));
    data_dir.join("aitop").join("pricing-cache.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_litellm_json() {
        let json = r#"{
            "claude-opus-4-6": {
                "input_cost_per_token": 0.000005,
                "output_cost_per_token": 0.000025,
                "cache_read_input_token_cost": 0.0000005,
                "cache_creation_input_token_cost": 0.00001,
                "max_tokens": 32000
            },
            "gpt-4o": {
                "input_cost_per_token": 0.0000025,
                "output_cost_per_token": 0.00001,
                "max_tokens": 16384
            },
            "no-pricing-model": {
                "max_tokens": 4096
            }
        }"#;

        let prices = parse_litellm_json(json).unwrap();

        // claude-opus-4-6
        let opus = prices.get("claude-opus-4-6").unwrap();
        assert!((opus.input - 5.0).abs() < 0.01);
        assert!((opus.output - 25.0).abs() < 0.01);
        assert!((opus.cache_read - 0.5).abs() < 0.01);
        assert!((opus.cache_creation - 10.0).abs() < 0.01);

        // gpt-4o (no cache costs → defaults)
        let gpt4o = prices.get("gpt-4o").unwrap();
        assert!((gpt4o.input - 2.5).abs() < 0.01);
        assert!((gpt4o.output - 10.0).abs() < 0.01);
        assert!((gpt4o.cache_read - 0.25).abs() < 0.01); // 10% of input
        assert!((gpt4o.cache_creation - 5.0).abs() < 0.01); // 2x input

        // no-pricing-model should be absent
        assert!(!prices.contains_key("no-pricing-model"));
    }

    #[test]
    fn test_load_cached_missing() {
        let path = Path::new("/tmp/nonexistent-aitop-cache.json");
        let result = load_cached(path).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_cache_path() {
        let path = cache_path();
        assert!(path.to_string_lossy().contains("aitop"));
        assert!(path.to_string_lossy().contains("pricing-cache.json"));
    }
}

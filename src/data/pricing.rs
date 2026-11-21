use std::collections::HashMap;

/// Per-million-token pricing for a model.
#[derive(Debug, Clone)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

#[derive(Clone)]
struct PricingRule {
    pattern: String,
    price: ModelPrice,
}

/// Registry of model pricing rules.
///
/// Lookup order: user overrides from `config.toml` (substring match), then an
/// exact model-id match in the dated `llm-prices` table, then the built-in
/// substring rules below, then Sonnet pricing for unknown models.
#[derive(Clone)]
pub struct PricingRegistry {
    overrides: Vec<PricingRule>,
    rules: Vec<PricingRule>,
    fallback: ModelPrice,
}

/// Convert an `llm-prices` row into aitop's per-million-token price.
///
/// Cache creation uses the 1-hour write rate when the provider lists one
/// (Claude Code writes 1h cache entries), otherwise the first listed write
/// rate, otherwise the base input rate (OpenAI and Gemini bill implicit cache
/// writes as ordinary input). A row without a listed input or output price
/// returns `None` so the built-in rules still apply.
fn from_table(p: &llm_prices::Price) -> Option<ModelPrice> {
    let input = p.input?;
    let output = p.output?;
    let cache_creation = p
        .cache_write("1h")
        .or_else(|| p.cache_write.first().map(|(_, v)| *v))
        .unwrap_or(input);
    Some(ModelPrice {
        input,
        output,
        cache_read: p.cache_read.unwrap_or(input),
        cache_creation,
    })
}

impl PricingRegistry {
    /// Create a registry with built-in pricing for all providers.
    ///
    /// Prices are per million tokens ($/MTok).
    /// Cache read = 0.1x base input. Cache creation = 2x base input (1-hour cache,
    /// which is what Claude Code uses — see `ephemeral_1h_input_tokens` in JSONL).
    pub fn builtin() -> Self {
        // Anthropic — Opus 4.5 / 4.6 (reduced pricing)
        let opus_new = ModelPrice {
            input: 5.0,
            output: 25.0,
            cache_read: 0.50,
            cache_creation: 10.0,
        };
        // Anthropic — Opus 4.0 / 4.1 (legacy pricing)
        let opus_legacy = ModelPrice {
            input: 15.0,
            output: 75.0,
            cache_read: 1.50,
            cache_creation: 30.0,
        };
        // Anthropic — Sonnet (all versions: 3.7, 4, 4.5, 4.6)
        let sonnet = ModelPrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.30,
            cache_creation: 6.0,
        };
        // Anthropic — Haiku 4.5
        let haiku_45 = ModelPrice {
            input: 1.0,
            output: 5.0,
            cache_read: 0.10,
            cache_creation: 2.0,
        };
        // Anthropic — Haiku 3.5
        let haiku_35 = ModelPrice {
            input: 0.80,
            output: 4.0,
            cache_read: 0.08,
            cache_creation: 1.60,
        };
        // Anthropic — Haiku 3 (deprecated)
        let haiku_3 = ModelPrice {
            input: 0.25,
            output: 1.25,
            cache_read: 0.03,
            cache_creation: 0.50,
        };

        // Google Gemini models
        let gemini_3_pro = ModelPrice {
            input: 2.50,
            output: 15.00,
            cache_read: 0.625,
            cache_creation: 5.0,
        };
        let gemini_25_pro = ModelPrice {
            input: 1.25,
            output: 10.00,
            cache_read: 0.3125,
            cache_creation: 2.50,
        };
        let gemini_25_flash = ModelPrice {
            input: 0.15,
            output: 0.60,
            cache_read: 0.0375,
            cache_creation: 0.30,
        };
        let gemini_20_flash = ModelPrice {
            input: 0.10,
            output: 0.40,
            cache_read: 0.025,
            cache_creation: 0.20,
        };

        // OpenAI models
        let gpt_4o = ModelPrice {
            input: 2.50,
            output: 10.00,
            cache_read: 1.25,
            cache_creation: 2.50,
        };
        let gpt_4o_mini = ModelPrice {
            input: 0.15,
            output: 0.60,
            cache_read: 0.075,
            cache_creation: 0.15,
        };
        let o3 = ModelPrice {
            input: 10.00,
            output: 40.00,
            cache_read: 2.50,
            cache_creation: 10.00,
        };
        let o4_mini = ModelPrice {
            input: 1.10,
            output: 4.40,
            cache_read: 0.275,
            cache_creation: 1.10,
        };

        // Rules are matched first-match-wins by substring. Order matters:
        // more specific patterns must come before general ones.
        let rules = vec![
            // Anthropic — Opus (specific versions before catch-all)
            PricingRule { pattern: "claude-opus-4-6".into(), price: opus_new.clone() },
            PricingRule { pattern: "claude-opus-4-5".into(), price: opus_new.clone() },
            PricingRule { pattern: "claude-opus-4-1".into(), price: opus_legacy.clone() },
            PricingRule { pattern: "claude-opus-4".into(), price: opus_legacy.clone() },
            PricingRule { pattern: "opus".into(), price: opus_new },
            // Anthropic — Haiku (specific versions before catch-all)
            PricingRule { pattern: "claude-haiku-4-5".into(), price: haiku_45.clone() },
            PricingRule { pattern: "claude-3-5-haiku".into(), price: haiku_35 },
            PricingRule { pattern: "claude-3-haiku".into(), price: haiku_3 },
            PricingRule { pattern: "haiku".into(), price: haiku_45 },
            // Anthropic — Sonnet
            PricingRule { pattern: "claude-sonnet-4".into(), price: sonnet.clone() },
            PricingRule { pattern: "claude-3-7-sonnet".into(), price: sonnet.clone() },
            PricingRule { pattern: "sonnet".into(), price: sonnet.clone() },
            // Gemini
            PricingRule { pattern: "gemini-3-pro".into(), price: gemini_3_pro },
            PricingRule { pattern: "gemini-2.5-pro".into(), price: gemini_25_pro },
            PricingRule { pattern: "gemini-2.5-flash".into(), price: gemini_25_flash },
            PricingRule { pattern: "gemini-2.0-flash".into(), price: gemini_20_flash },
            // OpenAI
            PricingRule { pattern: "gpt-4o-mini".into(), price: gpt_4o_mini },
            PricingRule { pattern: "gpt-4o".into(), price: gpt_4o },
            PricingRule { pattern: "o4-mini".into(), price: o4_mini },
            PricingRule { pattern: "o3".into(), price: o3 },
        ];

        PricingRegistry {
            overrides: Vec::new(),
            rules,
            fallback: sonnet,
        }
    }

    /// Create a registry with built-in pricing plus user overrides.
    /// User overrides are checked first (highest priority).
    pub fn with_overrides(overrides: &HashMap<String, ModelPriceConfig>) -> Self {
        let mut registry = Self::builtin();
        registry.overrides = overrides
            .iter()
            .map(|(pattern, cfg)| PricingRule {
                pattern: pattern.clone(),
                price: ModelPrice {
                    input: cfg.input,
                    output: cfg.output,
                    cache_read: cfg.cache_read,
                    cache_creation: cfg.cache_creation,
                },
            })
            .collect();
        registry
    }

    /// Look up pricing for a model string.
    ///
    /// User overrides win. Otherwise an exact id in the `llm-prices` table
    /// (dated snapshots, `-latest` and provider prefixes such as `openai/` or
    /// `vertex_ai/` resolve too) is used, so a new model is not priced at
    /// whichever substring rule it happens to contain. Ids the table does not
    /// know fall through to the substring rules and then the Sonnet fallback.
    pub fn lookup(&self, model: &str) -> ModelPrice {
        let model_lower = model.to_lowercase();
        if let Some(rule) = self
            .overrides
            .iter()
            .find(|r| model_lower.contains(&r.pattern))
        {
            return rule.price.clone();
        }
        if let Some(price) = llm_prices::lookup(&model_lower).and_then(from_table) {
            return price;
        }
        self.rules
            .iter()
            .find(|r| model_lower.contains(&r.pattern))
            .map_or_else(|| self.fallback.clone(), |r| r.price.clone())
    }

    /// Compute total cost for a message given token counts.
    pub fn compute_cost(
        &self,
        model: &str,
        input: i64,
        output: i64,
        cache_read: i64,
        cache_creation: i64,
    ) -> f64 {
        let price = self.lookup(model);
        let scale = 1_000_000.0;
        (input as f64 * price.input / scale)
            + (output as f64 * price.output / scale)
            + (cache_read as f64 * price.cache_read / scale)
            + (cache_creation as f64 * price.cache_creation / scale)
    }
}

/// Config-file representation of model pricing overrides.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ModelPriceConfig {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_creation: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_46_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-opus-4-6");
        assert_eq!(price.input, 5.0);
        assert_eq!(price.output, 25.0);
        assert_eq!(price.cache_read, 0.50);
        assert_eq!(price.cache_creation, 10.0);
    }

    #[test]
    fn test_opus_45_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-opus-4-5-20251101");
        assert_eq!(price.input, 5.0);
        assert_eq!(price.output, 25.0);
    }

    #[test]
    fn test_opus_4_legacy_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-opus-4-20250514");
        assert_eq!(price.input, 15.0);
        assert_eq!(price.output, 75.0);
        assert_eq!(price.cache_read, 1.50);
        assert_eq!(price.cache_creation, 30.0);
    }

    #[test]
    fn test_opus_41_legacy_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-opus-4-1-20250805");
        assert_eq!(price.input, 15.0);
        assert_eq!(price.output, 75.0);
    }

    #[test]
    fn test_bare_opus_uses_new_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("opus");
        assert_eq!(price.input, 5.0);
    }

    #[test]
    fn test_haiku_45_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-haiku-4-5-20251001");
        assert_eq!(price.input, 1.0);
        assert_eq!(price.output, 5.0);
        assert_eq!(price.cache_read, 0.10);
        assert_eq!(price.cache_creation, 2.0);
    }

    #[test]
    fn test_haiku_35_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-3-5-haiku-20241022");
        assert_eq!(price.input, 0.80);
        assert_eq!(price.output, 4.0);
    }

    #[test]
    fn test_haiku_3_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-3-haiku-20240307");
        assert_eq!(price.input, 0.25);
        assert_eq!(price.output, 1.25);
    }

    #[test]
    fn test_bare_haiku_uses_45_pricing() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("haiku");
        assert_eq!(price.input, 1.0);
    }

    #[test]
    fn test_builtin_sonnet() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-sonnet-4-6-20250514");
        assert_eq!(price.input, 3.0);
        assert_eq!(price.output, 15.0);
        assert_eq!(price.cache_read, 0.30);
        assert_eq!(price.cache_creation, 6.0);
    }

    #[test]
    fn test_builtin_37_sonnet() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-3-7-sonnet-20250219");
        assert_eq!(price.input, 3.0);
    }

    #[test]
    fn test_unknown_model_falls_back_to_sonnet() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("some-totally-unknown-model");
        assert_eq!(price.input, 3.0);
        assert_eq!(price.output, 15.0);
    }

    #[test]
    fn test_pricing_gemini_models() {
        let reg = PricingRegistry::builtin();

        let price = reg.lookup("gemini-3-pro-preview");
        assert_eq!(price.input, 2.50);
        assert_eq!(price.output, 15.00);

        // Current list prices from the llm-prices table: Gemini 2.5 cache
        // reads are 10% of input, and 2.5 Flash is 0.30/2.50, not 0.15/0.60.
        let price = reg.lookup("gemini-2.5-pro");
        assert_eq!(price.input, 1.25);
        assert_eq!(price.output, 10.00);
        assert_eq!(price.cache_read, 0.125);
        assert_eq!(price.cache_creation, 1.25);

        let price = reg.lookup("gemini-2.5-flash");
        assert_eq!(price.input, 0.30);
        assert_eq!(price.output, 2.50);
        assert_eq!(price.cache_read, 0.03);
        assert_eq!(price.cache_creation, 0.30);

        let price = reg.lookup("gemini-2.0-flash");
        assert_eq!(price.input, 0.10);
        assert_eq!(price.output, 0.40);
        assert_eq!(price.cache_read, 0.025);
        assert_eq!(price.cache_creation, 0.20);
    }

    #[test]
    fn test_pricing_openai_models() {
        let reg = PricingRegistry::builtin();

        let price = reg.lookup("gpt-4o");
        assert_eq!(price.input, 2.50);
        assert_eq!(price.output, 10.00);
        assert_eq!(price.cache_read, 1.25);
        assert_eq!(price.cache_creation, 2.50);

        let price = reg.lookup("gpt-4o-mini");
        assert_eq!(price.input, 0.15);
        assert_eq!(price.output, 0.60);
        assert_eq!(price.cache_read, 0.075);
        assert_eq!(price.cache_creation, 0.15);

        // o3 has been 2/8 since OpenAI's June 2025 price cut; 10/40 overstated it 5x.
        let price = reg.lookup("o3");
        assert_eq!(price.input, 2.00);
        assert_eq!(price.output, 8.00);
        assert_eq!(price.cache_read, 0.50);
        assert_eq!(price.cache_creation, 2.00);

        let price = reg.lookup("o4-mini");
        assert_eq!(price.input, 1.10);
        assert_eq!(price.output, 4.40);
        assert_eq!(price.cache_read, 0.275);
        assert_eq!(price.cache_creation, 1.10);
    }

    #[test]
    fn test_gpt5_and_gpt41_no_longer_fall_back_to_sonnet() {
        let reg = PricingRegistry::builtin();

        let price = reg.lookup("gpt-5");
        assert_eq!(price.input, 1.25);
        assert_eq!(price.output, 10.0);
        assert_eq!(price.cache_read, 0.125);

        let price = reg.lookup("gpt-5-mini-2025-08-07");
        assert_eq!(price.input, 0.25);
        assert_eq!(price.output, 2.0);

        let price = reg.lookup("gpt-4.1");
        assert_eq!(price.input, 2.0);
        assert_eq!(price.output, 8.0);

        let price = reg.lookup("openai/gpt-4.1-mini");
        assert_eq!(price.input, 0.4);
        assert_eq!(price.output, 1.6);
    }

    #[test]
    fn test_new_claude_families_get_their_own_price() {
        let reg = PricingRegistry::builtin();

        // The generic "sonnet" rule priced this at 3/15.
        let price = reg.lookup("claude-sonnet-5");
        assert_eq!(price.input, 2.0);
        assert_eq!(price.output, 10.0);
        assert_eq!(price.cache_creation, 4.0);

        // No substring rule matched this; it fell back to Sonnet 3/15.
        let price = reg.lookup("claude-fable-5");
        assert_eq!(price.input, 10.0);
        assert_eq!(price.output, 50.0);
        assert_eq!(price.cache_creation, 20.0);

        let price = reg.lookup("claude-opus-4-7");
        assert_eq!(price.input, 5.0);
        assert_eq!(price.output, 25.0);
    }

    #[test]
    fn test_compute_cost_gpt5_output() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("gpt-5", 0, 1_000_000, 0, 0);
        assert!((cost - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_user_override_beats_table() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "gpt-5".into(),
            ModelPriceConfig {
                input: 9.0,
                output: 9.0,
                cache_read: 9.0,
                cache_creation: 9.0,
            },
        );
        let reg = PricingRegistry::with_overrides(&overrides);
        assert_eq!(reg.lookup("gpt-5").input, 9.0);
    }

    #[test]
    fn test_compute_cost_sonnet_input_only() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("claude-sonnet-4-6-20250514", 1_000_000, 0, 0, 0);
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_cost_opus_46_output_only() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("claude-opus-4-6", 0, 1_000_000, 0, 0);
        assert!((cost - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_cost_opus_4_legacy_output() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("claude-opus-4-20250514", 0, 1_000_000, 0, 0);
        assert!((cost - 75.0).abs() < 0.001);
    }

    #[test]
    fn test_user_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "my-custom-model".into(),
            ModelPriceConfig {
                input: 5.0,
                output: 25.0,
                cache_read: 0.50,
                cache_creation: 6.25,
            },
        );
        let reg = PricingRegistry::with_overrides(&overrides);
        let price = reg.lookup("my-custom-model-v2");
        assert_eq!(price.input, 5.0);
        assert_eq!(price.output, 25.0);
    }

    #[test]
    fn test_user_override_does_not_break_builtins() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "my-model".into(),
            ModelPriceConfig {
                input: 99.0,
                output: 99.0,
                cache_read: 99.0,
                cache_creation: 99.0,
            },
        );
        let reg = PricingRegistry::with_overrides(&overrides);
        let price = reg.lookup("claude-opus-4-20250514");
        assert_eq!(price.input, 15.0);
    }
}

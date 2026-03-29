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

/// Registry of model pricing rules. Matches model names by substring,
/// most-specific-first. Falls back to Sonnet pricing for unknown models.
#[derive(Clone)]
pub struct PricingRegistry {
    rules: Vec<PricingRule>,
    fallback: ModelPrice,
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

        PricingRegistry { rules, fallback: sonnet }
    }

    /// Create a registry with built-in pricing plus user overrides.
    /// User overrides are checked first (highest priority).
    pub fn with_overrides(overrides: &HashMap<String, ModelPriceConfig>) -> Self {
        let mut registry = Self::builtin();
        let mut user_rules: Vec<PricingRule> = overrides
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
        user_rules.append(&mut registry.rules);
        registry.rules = user_rules;
        registry
    }

    /// Look up pricing for a model string. Matches the first rule whose
    /// pattern is a case-insensitive substring of the model name.
    pub fn lookup(&self, model: &str) -> &ModelPrice {
        let model_lower = model.to_lowercase();
        for rule in &self.rules {
            if model_lower.contains(&rule.pattern) {
                return &rule.price;
            }
        }
        &self.fallback
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

        let price = reg.lookup("gemini-2.5-pro");
        assert_eq!(price.input, 1.25);
        assert_eq!(price.output, 10.00);
        assert_eq!(price.cache_read, 0.3125);
        assert_eq!(price.cache_creation, 2.50);

        let price = reg.lookup("gemini-2.5-flash");
        assert_eq!(price.input, 0.15);
        assert_eq!(price.output, 0.60);
        assert_eq!(price.cache_read, 0.0375);
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

        let price = reg.lookup("o3");
        assert_eq!(price.input, 10.00);
        assert_eq!(price.output, 40.00);
        assert_eq!(price.cache_read, 2.50);
        assert_eq!(price.cache_creation, 10.00);

        let price = reg.lookup("o4-mini");
        assert_eq!(price.input, 1.10);
        assert_eq!(price.output, 4.40);
        assert_eq!(price.cache_read, 0.275);
        assert_eq!(price.cache_creation, 1.10);
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

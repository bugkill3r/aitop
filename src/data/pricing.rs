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
    pub fn builtin() -> Self {
        // Anthropic models
        let opus = ModelPrice {
            input: 15.0,
            output: 75.0,
            cache_read: 1.50,
            cache_creation: 18.75,
        };
        let haiku = ModelPrice {
            input: 0.80,
            output: 4.0,
            cache_read: 0.08,
            cache_creation: 1.0,
        };
        let sonnet = ModelPrice {
            input: 3.0,
            output: 15.0,
            cache_read: 0.30,
            cache_creation: 3.75,
        };

        // Google Gemini models
        let gemini_25_pro = ModelPrice {
            input: 1.25,
            output: 10.00,
            cache_read: 0.3125,
            cache_creation: 1.5625,
        };
        let gemini_25_flash = ModelPrice {
            input: 0.15,
            output: 0.60,
            cache_read: 0.0375,
            cache_creation: 0.1875,
        };
        let gemini_20_flash = ModelPrice {
            input: 0.10,
            output: 0.40,
            cache_read: 0.025,
            cache_creation: 0.125,
        };

        // OpenAI models
        let gpt_4o = ModelPrice {
            input: 2.50,
            output: 10.00,
            cache_read: 1.25,
            cache_creation: 3.125,
        };
        let gpt_4o_mini = ModelPrice {
            input: 0.15,
            output: 0.60,
            cache_read: 0.075,
            cache_creation: 0.1875,
        };
        let o3 = ModelPrice {
            input: 10.00,
            output: 40.00,
            cache_read: 1.00,
            cache_creation: 12.50,
        };
        let o4_mini = ModelPrice {
            input: 1.10,
            output: 4.40,
            cache_read: 0.275,
            cache_creation: 1.375,
        };

        let rules = vec![
            // Anthropic
            PricingRule { pattern: "claude-opus-4".into(), price: opus.clone() },
            PricingRule { pattern: "opus".into(), price: opus },
            PricingRule { pattern: "claude-3-5-haiku".into(), price: haiku.clone() },
            PricingRule { pattern: "haiku".into(), price: haiku },
            PricingRule { pattern: "claude-sonnet-4".into(), price: sonnet.clone() },
            PricingRule { pattern: "claude-3-7-sonnet".into(), price: sonnet.clone() },
            PricingRule { pattern: "sonnet".into(), price: sonnet.clone() },
            // Gemini
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_opus() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-opus-4-20250514");
        assert_eq!(price.input, 15.0);
        assert_eq!(price.output, 75.0);
        assert_eq!(price.cache_read, 1.50);
        assert_eq!(price.cache_creation, 18.75);
    }

    #[test]
    fn test_builtin_haiku() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-3-5-haiku-20241022");
        assert_eq!(price.input, 0.80);
        assert_eq!(price.output, 4.0);
    }

    #[test]
    fn test_builtin_sonnet() {
        let reg = PricingRegistry::builtin();
        let price = reg.lookup("claude-sonnet-4-6-20250514");
        assert_eq!(price.input, 3.0);
        assert_eq!(price.output, 15.0);
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

        // gemini-2.5-pro
        let price = reg.lookup("gemini-2.5-pro");
        assert_eq!(price.input, 1.25);
        assert_eq!(price.output, 10.00);
        assert_eq!(price.cache_read, 0.3125);
        assert_eq!(price.cache_creation, 1.5625);

        // gemini-2.5-flash
        let price = reg.lookup("gemini-2.5-flash");
        assert_eq!(price.input, 0.15);
        assert_eq!(price.output, 0.60);
        assert_eq!(price.cache_read, 0.0375);
        assert_eq!(price.cache_creation, 0.1875);

        // gemini-2.0-flash
        let price = reg.lookup("gemini-2.0-flash");
        assert_eq!(price.input, 0.10);
        assert_eq!(price.output, 0.40);
        assert_eq!(price.cache_read, 0.025);
        assert_eq!(price.cache_creation, 0.125);
    }

    #[test]
    fn test_pricing_openai_models() {
        let reg = PricingRegistry::builtin();

        // gpt-4o
        let price = reg.lookup("gpt-4o");
        assert_eq!(price.input, 2.50);
        assert_eq!(price.output, 10.00);
        assert_eq!(price.cache_read, 1.25);
        assert_eq!(price.cache_creation, 3.125);

        // gpt-4o-mini
        let price = reg.lookup("gpt-4o-mini");
        assert_eq!(price.input, 0.15);
        assert_eq!(price.output, 0.60);
        assert_eq!(price.cache_read, 0.075);
        assert_eq!(price.cache_creation, 0.1875);

        // o3
        let price = reg.lookup("o3");
        assert_eq!(price.input, 10.00);
        assert_eq!(price.output, 40.00);
        assert_eq!(price.cache_read, 1.00);
        assert_eq!(price.cache_creation, 12.50);

        // o4-mini
        let price = reg.lookup("o4-mini");
        assert_eq!(price.input, 1.10);
        assert_eq!(price.output, 4.40);
        assert_eq!(price.cache_read, 0.275);
        assert_eq!(price.cache_creation, 1.375);
    }

    #[test]
    fn test_compute_cost_sonnet_input_only() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("claude-sonnet-4-6-20250514", 1_000_000, 0, 0, 0);
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_cost_opus_output_only() {
        let reg = PricingRegistry::builtin();
        let cost = reg.compute_cost("claude-opus-4-20250514", 0, 1_000_000, 0, 0);
        assert!((cost - 75.0).abs() < 0.001);
    }
}

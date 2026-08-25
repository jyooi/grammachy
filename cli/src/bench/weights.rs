//! What a model's weights may be used for, spec section 13.1.
//!
//! The Models table carries a weights license column, and the recommended model
//! of the Settings defaults and the README is re-decided from that table on
//! every tag. The spec fixes two rules:
//!
//! - The recommended model must be Apache-2.0 or MIT licensed.
//! - A model with non-commercial weights may appear for reference but is never
//!   recommended.
//!
//! So this table is a product rule, not a convenience. A model it does not know
//! is `Unknown`, which is never recommended either: a license nobody checked is
//! not a license that passed.

/// What one license allows, as far as the recommendation rule cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terms {
    /// Apache-2.0 or MIT: the only class the recommended model may come from.
    Permissive,
    /// Commercial use is allowed, but the license is neither Apache-2.0 nor MIT.
    Restricted,
    /// Research or non-production only.
    NonCommercial,
    /// Not in the table below.
    Unknown,
}

/// The weights license of one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    /// The license name, as the model card writes it.
    pub license: &'static str,
    pub terms: Terms,
}

impl Weights {
    /// The Recommended cell of the Models table.
    pub fn recommendation(&self) -> &'static str {
        match self.terms {
            Terms::Permissive => "eligible",
            Terms::Restricted => "no, the license is neither Apache-2.0 nor MIT",
            Terms::NonCommercial => "never, the weights are non-commercial",
            Terms::Unknown => "no, the license was not checked",
        }
    }
}

/// The known model families, longest prefix first.
///
/// A model name in Settings carries the quantisation, as in
/// `qwen2.5-7b-instruct-q4_k_m`, so the match is on the prefix and ignores case.
const KNOWN: &[(&str, Weights)] = &[
    // Alibaba released Qwen2.5 under Apache-2.0 except the 3B and the 72B.
    (
        "qwen2.5-3b",
        Weights {
            license: "Qwen Research License",
            terms: Terms::NonCommercial,
        },
    ),
    (
        "qwen2.5-72b",
        Weights {
            license: "Qwen License",
            terms: Terms::Restricted,
        },
    ),
    (
        "qwen2.5",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    (
        "qwen3",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    (
        "gemma",
        Weights {
            license: "Gemma Terms of Use",
            terms: Terms::Restricted,
        },
    ),
    (
        "llama-3",
        Weights {
            license: "Llama Community License",
            terms: Terms::Restricted,
        },
    ),
    (
        "phi-4",
        Weights {
            license: "MIT",
            terms: Terms::Permissive,
        },
    ),
    (
        "phi-3",
        Weights {
            license: "MIT",
            terms: Terms::Permissive,
        },
    ),
    (
        "mistral-nemo",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    (
        "codestral",
        Weights {
            license: "Mistral AI Non-Production License",
            terms: Terms::NonCommercial,
        },
    ),
];

const UNKNOWN: Weights = Weights {
    license: "unknown",
    terms: Terms::Unknown,
};

/// The weights license of one model name.
pub fn of(model: &str) -> Weights {
    let wanted = model.trim().to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(prefix, _)| wanted.starts_with(prefix))
        .map(|(_, weights)| *weights)
        .unwrap_or(UNKNOWN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_permissive_model_is_eligible_for_the_default() {
        let weights = of("Qwen2.5-7B-Instruct-Q4_K_M");

        assert_eq!(weights.license, "Apache-2.0");
        assert_eq!(weights.terms, Terms::Permissive);
        assert_eq!(weights.recommendation(), "eligible");
    }

    #[test]
    fn a_model_with_non_commercial_weights_is_never_recommended() {
        for model in [
            "qwen2.5-3b-instruct",
            "Qwen2.5-3B-Instruct-Q4_K_M",
            "codestral-22b",
        ] {
            let weights = of(model);

            assert_eq!(weights.terms, Terms::NonCommercial, "{model}");
            assert!(
                weights.recommendation().starts_with("never"),
                "{model} is marked never recommended: {}",
                weights.recommendation()
            );
        }
    }

    #[test]
    fn the_longer_prefix_wins_over_the_family() {
        assert_eq!(of("qwen2.5-3b-instruct").terms, Terms::NonCommercial);
        assert_eq!(of("qwen2.5-72b-instruct").terms, Terms::Restricted);
        assert_eq!(of("qwen2.5-7b-instruct").terms, Terms::Permissive);
    }

    #[test]
    fn a_license_that_allows_commercial_use_but_is_not_apache_or_mit_is_not_eligible() {
        let weights = of(crate::settings::DEFAULT_OPENAI_MODEL);

        assert_eq!(weights.terms, Terms::Restricted);
        assert_eq!(
            weights.recommendation(),
            "no, the license is neither Apache-2.0 nor MIT"
        );
    }

    #[test]
    fn an_unknown_model_is_not_recommended_either() {
        let weights = of("some-model-nobody-checked");

        assert_eq!(weights.license, "unknown");
        assert_eq!(weights.terms, Terms::Unknown);
        assert_eq!(weights.recommendation(), "no, the license was not checked");
    }
}

//! What a model's weights may be used for, `docs/spec/evals.md` section 5.
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
//! not a license that passed. A model reached through the cloud engine has no
//! weights on this machine at all, so it is `Hosted` and competes only for the
//! cloud recommendation line (HUF-206).

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
    /// Served by a provider through the cloud engine; no weights to license.
    Hosted,
}

/// The weights license of one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    /// The license name, as the model card writes it.
    pub license: &'static str,
    pub terms: Terms,
}

impl Weights {
    /// Whether the license alone allows the row to be recommended.
    pub fn is_eligible(&self) -> bool {
        matches!(self.terms, Terms::Permissive | Terms::Hosted)
    }

    /// Why the license keeps the row out of the recommendation, or `None`.
    pub fn objection(&self) -> Option<&'static str> {
        match self.terms {
            Terms::Permissive | Terms::Hosted => None,
            Terms::Restricted => Some("no, the license is neither Apache-2.0 nor MIT"),
            Terms::NonCommercial => Some("never, the weights are non-commercial"),
            Terms::Unknown => Some("no, the license was not checked"),
        }
    }
}

/// The known model families, longest prefix first.
///
/// A model name in Settings carries the quantisation, as in
/// `qwen2.5-7b-instruct-q4_k_m`, so the match is on the prefix and ignores case.
/// A prefix matches only when the name ends there or the next character is a
/// hyphen. A longer family name does not take the terms of a shorter family.
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
    // Every Qwen3 release so far, including the 3.5 line, is Apache-2.0.
    (
        "qwen3",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    // Gemma 4 moved to Apache-2.0 (HUF-204); earlier Gemma stays on its terms.
    (
        "gemma-4",
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
        "ministral",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    (
        "granite",
        Weights {
            license: "Apache-2.0",
            terms: Terms::Permissive,
        },
    ),
    (
        "smollm3",
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

/// The Weights column of a row that runs through the cloud engine.
pub const HOSTED: Weights = Weights {
    license: "hosted",
    terms: Terms::Hosted,
};

/// A prefix matches at the end of the name or before a hyphen or a dot, so
/// `qwen3` covers `qwen3.5-4b` and `gemma-4` covers `gemma-4-e4b-it` while
/// `gemma` does not swallow `gemma2-9b`.
fn matches_prefix(name: &str, prefix: &str) -> bool {
    if !name.starts_with(prefix) {
        return false;
    }
    match name.as_bytes().get(prefix.len()) {
        None => true,
        Some(next) => *next == b'-' || *next == b'.',
    }
}

/// The weights license of one model name.
pub fn of(model: &str) -> Weights {
    let wanted = model.trim().to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(prefix, _)| matches_prefix(&wanted, prefix))
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
        assert!(weights.is_eligible());
        assert_eq!(weights.objection(), None);
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
                weights
                    .objection()
                    .is_some_and(|why| why.starts_with("never")),
                "{model} is marked never recommended: {:?}",
                weights.objection()
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
    fn the_shipped_default_is_apache_licensed_and_eligible() {
        let weights = of(crate::settings::DEFAULT_OPENAI_MODEL);

        assert_eq!(weights.license, "Apache-2.0");
        assert!(weights.is_eligible());
    }

    #[test]
    fn earlier_gemma_keeps_its_own_terms() {
        let weights = of("gemma-3-4b-it");

        assert_eq!(weights.terms, Terms::Restricted);
        assert_eq!(
            weights.objection(),
            Some("no, the license is neither Apache-2.0 nor MIT")
        );
    }

    #[test]
    fn the_qwen3_line_matches_through_its_point_release() {
        assert_eq!(of("Qwen3.5-4B-Q4_K_M").terms, Terms::Permissive);
        assert_eq!(of("qwen3-8b").terms, Terms::Permissive);
    }

    #[test]
    fn the_small_apache_rows_of_the_candidate_list_are_known() {
        for model in [
            "Ministral-3-3B-Instruct-2512",
            "granite-4.1-3b",
            "SmolLM3-3B",
        ] {
            assert_eq!(of(model).license, "Apache-2.0", "{model}");
        }
    }

    #[test]
    fn an_unknown_model_is_not_recommended_either() {
        let weights = of("some-model-nobody-checked");

        assert_eq!(weights.license, "unknown");
        assert_eq!(weights.terms, Terms::Unknown);
        assert_eq!(weights.objection(), Some("no, the license was not checked"));
    }

    #[test]
    fn a_longer_family_name_does_not_inherit_a_shorter_prefix() {
        let weights = of("gemma2-9b");

        assert_eq!(weights.license, "unknown");
        assert_eq!(weights.terms, Terms::Unknown);
    }

    #[test]
    fn a_hosted_row_has_no_license_objection() {
        assert!(HOSTED.is_eligible());
        assert_eq!(HOSTED.license, "hosted");
    }
}

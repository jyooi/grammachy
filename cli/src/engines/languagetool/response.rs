//! The `/v2/check` response of the LanguageTool HTTP server, and the mapping
//! from its matches to the Issues of spec section 5.1.

use serde::Deserialize;

use crate::envelope::{Category, Issue};
use crate::text::utf16_slice;

/// Only the fields the adapter reads. LanguageTool sends many more.
#[derive(Debug, Clone, Deserialize)]
pub struct CheckResponse {
    #[serde(default)]
    pub matches: Vec<Match>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Match {
    /// Start of the match in UTF-16 code units, indexed into the sent text.
    pub offset: usize,
    /// Length of the match in UTF-16 code units.
    pub length: usize,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub replacements: Vec<Replacement>,
    #[serde(default)]
    pub rule: Option<Rule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Replacement {
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "issueType", default)]
    pub issue_type: String,
    #[serde(default)]
    pub category: Option<RuleCategory>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleCategory {
    #[serde(default)]
    pub id: String,
}

/// What one match is about. Depth in v1 is grammar and spelling only, so a
/// style match is reported to nobody (spec section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Depth {
    Grammar,
    Spelling,
    Style,
}

/// LanguageTool `issueType` values that are style rather than a mistake.
const STYLE_ISSUE_TYPES: [&str; 5] = [
    "style",
    "locale-violation",
    "register",
    "formatting",
    "non-conformance",
];

/// LanguageTool category ids that hold style rules.
const STYLE_CATEGORY_IDS: [&str; 7] = [
    "STYLE",
    "REDUNDANCY",
    "PLAIN_ENGLISH",
    "WORDINESS",
    "COLLOQUIALISMS",
    "CREATIVE_WRITING",
    "MISC_STYLE",
];

fn depth_of(rule: Option<&Rule>) -> Depth {
    let Some(rule) = rule else {
        return Depth::Grammar;
    };
    let category_id = rule.category.as_ref().map(|c| c.id.as_str()).unwrap_or("");

    if STYLE_ISSUE_TYPES.contains(&rule.issue_type.as_str())
        || STYLE_CATEGORY_IDS.contains(&category_id)
    {
        return Depth::Style;
    }
    if rule.issue_type == "misspelling" || category_id == "TYPOS" {
        return Depth::Spelling;
    }
    Depth::Grammar
}

/// One English sentence with no rule id (spec section 5.1).
///
/// LanguageTool messages are already written as prose. A few rules append their
/// own id, so it is removed, and the whitespace is collapsed to one space so a
/// wrapped message stays one line in the inspector.
fn reason_of(message: &str, rule_id: &str) -> String {
    let stripped = if rule_id.is_empty() {
        message.to_string()
    } else {
        message.replace(rule_id, "")
    };
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    // Removing the id can leave an empty bracket pair behind.
    collapsed
        .replace("( )", "")
        .replace("()", "")
        .replace("[]", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turn one match into an Issue, or drop it.
///
/// A match is dropped when it is style, when its span does not address the sent
/// text, when it carries no suggestion, or when its first suggestion is the
/// text that is already there.
fn issue_of(text: &str, item: &Match) -> Option<Issue> {
    let category = match depth_of(item.rule.as_ref()) {
        Depth::Style => return None,
        Depth::Grammar => Category::Grammar,
        Depth::Spelling => Category::Spelling,
    };

    let start = item.offset;
    let end = item.offset.checked_add(item.length)?;
    let original = utf16_slice(text, start, end)?;

    let fix = item.replacements.first()?.value.clone();
    if fix == original {
        return None;
    }

    let rule_id = item.rule.as_ref().map(|rule| rule.id.clone());
    let reason = reason_of(&item.message, rule_id.as_deref().unwrap_or(""));

    Some(Issue {
        start,
        end,
        original: original.to_string(),
        fix,
        reason,
        category,
        rule_id: rule_id.filter(|id| !id.is_empty()),
    })
}

/// Map a whole response to the Issue list the envelope carries.
///
/// The result keeps every guarantee of spec section 5.1: sorted by `start`,
/// never overlapping because the earlier Issue wins, and never carrying a `fix`
/// equal to the `original`.
pub fn issues_from(text: &str, response: &CheckResponse) -> Vec<Issue> {
    let mut issues: Vec<Issue> = response
        .matches
        .iter()
        .filter_map(|item| issue_of(text, item))
        .collect();

    // A shorter span first keeps the tighter of two Issues that start together.
    issues.sort_by_key(|issue| (issue.start, issue.end));

    let mut kept: Vec<Issue> = Vec::with_capacity(issues.len());
    for issue in issues {
        match kept.last() {
            Some(previous) if issue.start < previous.end => continue,
            _ => kept.push(issue),
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_prose_and_loses_the_rule_id() {
        assert_eq!(
            reason_of("Possible agreement error.  (CD_NN)", "CD_NN"),
            "Possible agreement error."
        );
        assert_eq!(
            reason_of("Did you mean\n'books'?", "CD_NN"),
            "Did you mean 'books'?"
        );
    }

    #[test]
    fn a_style_match_is_dropped() {
        let rule = Rule {
            id: "TOO_LONG_SENTENCE".to_string(),
            issue_type: "style".to_string(),
            category: Some(RuleCategory {
                id: "STYLE".to_string(),
            }),
        };
        assert_eq!(depth_of(Some(&rule)), Depth::Style);
    }

    #[test]
    fn a_typo_is_spelling_and_anything_else_is_grammar() {
        let typo = Rule {
            id: "MORFOLOGIK_RULE_EN_US".to_string(),
            issue_type: "misspelling".to_string(),
            category: Some(RuleCategory {
                id: "TYPOS".to_string(),
            }),
        };
        let agreement = Rule {
            id: "CD_NN".to_string(),
            issue_type: "grammar".to_string(),
            category: Some(RuleCategory {
                id: "GRAMMAR".to_string(),
            }),
        };
        assert_eq!(depth_of(Some(&typo)), Depth::Spelling);
        assert_eq!(depth_of(Some(&agreement)), Depth::Grammar);
        assert_eq!(depth_of(None), Depth::Grammar);
    }
}

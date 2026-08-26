//! The opt-in cloud engine: any model on OpenRouter, chosen by id.
//!
//! Spec section 4 as amended by HUF-206. This is the one engine that sends a
//! Check off the machine, and it sends it to one constant endpoint on
//! openrouter.ai and nowhere else. It reuses the `openai` request body, JSON
//! schema, prompt, and response mapping, adds the OpenRouter fields, and reads
//! `usage.cost` so the benchmark can price a Check. The key lives in a 0600
//! file under `~/.config/grammachy/`, never in `shell.json`.
//!
//! The loopback rule of the `openai` engine is untouched: `endpoint::parse` is
//! not consulted here because there is no base URL to parse.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use crate::args::CheckOptions;
use crate::engine::{Answer, Engine, EngineFailure};
use crate::engines::openai::{prompt, response};

/// The one address a Check may leave the machine for.
pub const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

/// The Check timeout of spec section 4 for this engine.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Points the adapter at another key file, so tests never read the real one.
/// Not a user-facing setting.
pub const KEY_FILE_ENV: &str = "GRAMMACHY_OPENROUTER_KEY_FILE";

/// Points the adapter at a stub endpoint. A test seam only: the product has
/// no base URL setting for this engine, by decision (HUF-206).
pub const URL_ENV: &str = "GRAMMACHY_OPENROUTER_URL";

/// The key file under the user's home, spec section 4.
pub fn default_key_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".config/grammachy/openrouter-key"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub timeout: Duration,
    pub key_file: Option<PathBuf>,
    pub url: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            timeout: DEFAULT_TIMEOUT,
            key_file: default_key_file(),
            url: ENDPOINT.to_string(),
        }
    }
}

impl Config {
    /// Apply the two test seams.
    pub fn from_env() -> Self {
        let mut config = Config::default();
        if let Some(path) = std::env::var_os(KEY_FILE_ENV).filter(|path| !path.is_empty()) {
            config.key_file = Some(PathBuf::from(path));
        }
        if let Some(url) = std::env::var_os(URL_ENV) {
            let url = url.to_string_lossy().trim().to_string();
            if !url.is_empty() {
                config.url = url;
            }
        }
        config
    }
}

/// Whether a provider honours `temperature`. The GPT-5 line, Sonnet 5, and
/// Gemini 3.7 Flash reject or ignore it (HUF-204), so the field stays out of
/// their requests rather than earning a 400.
pub fn honours_temperature(model: &str) -> bool {
    let id = model.to_ascii_lowercase();
    !(id.starts_with("openai/")
        || id.starts_with("google/")
        || id.starts_with("anthropic/claude-sonnet"))
}

/// The `reasoning` field for one model: off where the provider allows it, and
/// the least it accepts where it does not. Gemini answers HTTP 400 "Reasoning
/// is mandatory for this endpoint" to `enabled: false` (pilot, 2026-08-26),
/// and `effort: minimal` is the smallest it takes.
pub fn reasoning(model: &str) -> Value {
    if model.to_ascii_lowercase().starts_with("google/") {
        json!({ "effort": "minimal" })
    } else {
        json!({ "enabled": false })
    }
}

/// The whole POST body: the `openai` body with the OpenRouter additions.
pub fn request_body(text: &str, options: &CheckOptions) -> Value {
    let local = CheckOptions {
        openai_model: options.openrouter_model.clone(),
        ..options.clone()
    };
    let mut body = prompt::request_body(text, &local);
    if let Some(fields) = body.as_object_mut() {
        fields.insert("usage".to_string(), json!({ "include": true }));
        fields.insert(
            "reasoning".to_string(),
            reasoning(&options.openrouter_model),
        );
        if !honours_temperature(&options.openrouter_model) {
            fields.remove("temperature");
        }
    }
    body
}

/// The key, trimmed to its one line, or the `no_key` failure.
fn read_key(config: &Config) -> Result<String, EngineFailure> {
    let Some(path) = &config.key_file else {
        return Err(EngineFailure::Unavailable(
            "Cloud LLM has no key: HOME is not set. (reason: no_key)".to_string(),
        ));
    };
    match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => Err(EngineFailure::Unavailable(
            "Cloud LLM has no key. Run: printf '%s' \"$KEY\" | grammachy setup --openrouter-key (reason: no_key)"
                .to_string(),
        )),
    }
}

pub struct Openrouter {
    config: Config,
}

impl Openrouter {
    pub fn new(config: Config) -> Self {
        Openrouter { config }
    }

    fn request(&self, key: &str, body: &str) -> Result<Value, EngineFailure> {
        // Redirects and proxies stay off, so the text goes to openrouter.ai
        // and nowhere the response or the environment could send it.
        // A status is read here rather than raised by ureq, so the error body
        // OpenRouter sends with a 400 reaches the message the card shows.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.config.timeout))
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();

        let answer = agent
            .post(&self.config.url)
            .content_type("application/json")
            .header("Authorization", &format!("Bearer {key}"))
            .header("X-Title", "Grammachy")
            .send(body)
            .map_err(|error| self.classify(error))?;

        let status = answer.status().as_u16();
        let text = answer
            .into_body()
            .read_to_string()
            .map_err(|error| EngineFailure::Failed(format!("OpenRouter sent no body: {error}")))?;
        if status != 200 {
            return Err(classify_status(status, &text));
        }

        serde_json::from_str(&text).map_err(|error| {
            EngineFailure::Failed(format!(
                "OpenRouter sent an answer that is not a chat completion: {error}"
            ))
        })
    }

    fn classify(&self, error: ureq::Error) -> EngineFailure {
        match error {
            ureq::Error::Timeout(_) => EngineFailure::Timeout(format!(
                "OpenRouter did not answer within {} s.",
                self.config.timeout.as_secs()
            )),
            ureq::Error::Io(_)
            | ureq::Error::ConnectionFailed
            | ureq::Error::HostNotFound
            | ureq::Error::Tls(_) => EngineFailure::Unavailable(
                "Cloud LLM is not reachable. Grammachy could not reach openrouter.ai. (reason: unreachable)"
                    .to_string(),
            ),
            other => EngineFailure::Failed(format!("OpenRouter could not be reached: {other}")),
        }
    }
}

/// The failure one non-200 status stands for, with the sentence OpenRouter
/// put in its error body when there is one.
fn classify_status(status: u16, body: &str) -> EngineFailure {
    let said = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    match status {
        401 | 403 => EngineFailure::Unavailable(
            "OpenRouter rejected the key. Run: printf '%s' \"$KEY\" | grammachy setup --openrouter-key (reason: rejected_key)"
                .to_string(),
        ),
        402 => EngineFailure::Unavailable(
            "OpenRouter credits are used up. Add credits on openrouter.ai, then retry. (reason: no_credit)"
                .to_string(),
        ),
        429 => EngineFailure::Unavailable(
            "OpenRouter is rate limited. Wait a moment, then retry. (reason: rate_limited)"
                .to_string(),
        ),
        400 | 404 if said.is_empty() => EngineFailure::BadArguments(
            "OpenRouter does not know the model, or refused the request for it.".to_string(),
        ),
        400 | 404 => EngineFailure::BadArguments(format!("OpenRouter refused the request: {said}")),
        _ if said.is_empty() => {
            EngineFailure::Failed(format!("OpenRouter answered with HTTP {status}."))
        }
        _ => EngineFailure::Failed(format!("OpenRouter answered with HTTP {status}: {said}")),
    }
}

impl Engine for Openrouter {
    fn slug(&self) -> &'static str {
        "openrouter"
    }

    fn check(
        &self,
        text: &str,
        options: &CheckOptions,
    ) -> Result<Vec<crate::envelope::Issue>, EngineFailure> {
        self.answer(text, options).map(|answer| answer.issues)
    }

    fn answer(&self, text: &str, options: &CheckOptions) -> Result<Answer, EngineFailure> {
        if options.openrouter_model.trim().is_empty() {
            return Err(EngineFailure::BadArguments(
                "The cloud model is not set.".to_string(),
            ));
        }
        // Before anything is sent: no key means no request.
        let key = read_key(&self.config)?;

        let body = request_body(text, options).to_string();
        let raw = self.request(&key, &body)?;

        let cost = raw
            .get("usage")
            .and_then(|usage| usage.get("cost"))
            .and_then(Value::as_f64);
        let completion: response::ChatResponse = serde_json::from_value(raw).map_err(|error| {
            EngineFailure::Failed(format!(
                "OpenRouter sent an answer that is not a chat completion: {error}"
            ))
        })?;
        let issues = response::issues_from(text, &completion).map_err(EngineFailure::Failed)?;

        Ok(Answer { issues, cost })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(model: &str) -> CheckOptions {
        CheckOptions {
            openrouter_model: model.to_string(),
            ..CheckOptions::default()
        }
    }

    #[test]
    fn the_body_adds_usage_and_disables_reasoning() {
        let body = request_body("He go home.", &options("deepseek/deepseek-v4-flash-0731"));

        assert_eq!(body["model"], "deepseek/deepseek-v4-flash-0731");
        assert_eq!(body["usage"]["include"], true);
        assert_eq!(body["reasoning"]["enabled"], false);
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["response_format"]["type"], "json_schema");
    }

    #[test]
    fn providers_that_refuse_temperature_do_not_receive_it() {
        for model in [
            "google/gemini-3.7-flash",
            "openai/gpt-5.4-nano",
            "anthropic/claude-sonnet-5",
        ] {
            let body = request_body("He go home.", &options(model));
            assert!(body.get("temperature").is_none(), "{model}");
        }
        assert!(request_body("x", &options("anthropic/claude-haiku-4.5"))
            .get("temperature")
            .is_some());
    }

    #[test]
    fn a_provider_that_cannot_disable_reasoning_gets_the_least_of_it() {
        let gemini = request_body("x", &options("google/gemini-3.7-flash"));
        assert_eq!(gemini["reasoning"], json!({ "effort": "minimal" }));

        let deepseek = request_body("x", &options("deepseek/deepseek-v4-flash-0731"));
        assert_eq!(deepseek["reasoning"], json!({ "enabled": false }));
    }

    #[test]
    fn a_refusal_carries_the_sentence_openrouter_sent() {
        let failure = classify_status(
            400,
            r#"{"error":{"message":"Reasoning is mandatory for this endpoint and cannot be disabled.","code":400}}"#,
        );
        assert!(
            matches!(&failure, EngineFailure::BadArguments(message) if message.contains("Reasoning is mandatory")),
            "{failure:?}"
        );
        assert!(matches!(
            classify_status(402, ""),
            EngineFailure::Unavailable(_)
        ));
        assert!(matches!(
            classify_status(503, "not json"),
            EngineFailure::Failed(_)
        ));
    }

    #[test]
    fn the_endpoint_is_constant_and_on_openrouter() {
        assert_eq!(Config::default().url, ENDPOINT);
        assert!(ENDPOINT.starts_with("https://openrouter.ai/"));
    }
}

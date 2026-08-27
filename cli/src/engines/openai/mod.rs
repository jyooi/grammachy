//! The opt-in deep engine: any OpenAI-compatible chat endpoint on this machine.
//!
//! Spec section 4. The adapter posts one chat completion to the base URL from
//! Settings and maps the answer to Issues. The recommended server is llama.cpp
//! with the model HUF-181 measured, and when the base URL is a loopback address
//! that does not answer, the adapter starts the transient user unit
//! `grammachy-llama` itself, exactly as the `languagetool` adapter does.
//!
//! The host rule is the product guarantee of this engine: `localhost`,
//! `127.0.0.1`, or `::1` and nothing else, so a Check never leaves the machine.
//! A base URL naming any other host is `bad_arguments`, and no request is made.
//!
//! The Local thinking Setting picks the forcing route, and [`force_of`] is the
//! one place that decides it. A raw grammar bounds the whole generation, so it
//! leaves a thinking model no room to think. Thinking on therefore keeps the
//! `json_schema` response format, and thinking off takes the grammar and the
//! compact answer of HUF-219.

pub mod endpoint;
pub mod prompt;
pub mod response;
pub mod unit;

use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::args::CheckOptions;
use crate::engine::{Answer, Engine, EngineFailure, Usage};
use crate::engines::local::{is_unreachable, StartFailure};
use crate::envelope::Issue;

use endpoint::Endpoint;
use response::ChatResponse;

/// The Check timeout of spec section 4.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90);

/// How long the adapter waits for a freshly started server to answer.
///
/// This budget is separate from the Check timeout, which applies to one
/// request. llama.cpp binds the port first and answers HTTP 503 until the
/// weights are loaded. The recommended model is a 4.7 GB file, so a first Check
/// after a login waits while the server reads it from disk.
pub const DEFAULT_STARTUP_BUDGET: Duration = Duration::from_secs(120);

/// Time between two probes while the unit starts.
const PROBE_INTERVAL: Duration = Duration::from_millis(500);

/// Keeps the adapter from starting a unit. The test suite and CI set it, so no
/// test ever touches systemd. Not a user-facing setting; settings live in
/// `shell.json` (spec section 7).
pub const START_ENV: &str = "GRAMMACHY_LLAMA_START";

/// Which forcing route one Check takes, from the Local thinking Setting.
///
/// The two accepted contracts ask for different things of the same request.
/// HUF-224 and HUF-225 want a think, and HUF-219 wants the compact answer a
/// raw grammar forces. A grammar bounds the whole generation, so no think fits
/// inside it. Thinking on therefore keeps the `json_schema` response format,
/// which leaves the think possible, and thinking off takes the grammar.
pub fn force_of(options: &CheckOptions) -> prompt::Force {
    match options.local_thinking {
        true => prompt::Force::JsonSchema,
        false => prompt::Force::Grammar,
    }
}

/// How long the adapter waits, and whether it may start a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Timeout of one request.
    pub timeout: Duration,
    /// Whether an unanswered loopback port makes the adapter start the unit.
    pub start_unit: bool,
    /// How long to wait for a started unit to answer.
    pub startup_budget: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            timeout: DEFAULT_TIMEOUT,
            start_unit: true,
            startup_budget: DEFAULT_STARTUP_BUDGET,
        }
    }
}

impl Config {
    /// Apply the test seam `GRAMMACHY_LLAMA_START=never`.
    pub fn from_env() -> Self {
        let mut config = Config::default();
        if std::env::var_os(START_ENV).is_some_and(|value| value == "never") {
            config.start_unit = false;
        }
        config
    }
}

/// What starts the server when the port does not answer.
///
/// The real one is [`unit::start`]. Tests hand in their own, which is how the
/// adapter's start behaviour is covered without a systemd unit.
pub type Starter = Box<dyn Fn(&str, &Endpoint) -> Result<(), StartFailure> + Send + Sync>;

pub struct Openai {
    config: Config,
    starter: Starter,
}

impl Openai {
    pub fn new(config: Config) -> Self {
        Openai {
            config,
            starter: Box::new(|model, endpoint| {
                unit::start(model, endpoint.bind_host(), endpoint.port)
            }),
        }
    }

    /// The adapter with another way to start the server.
    pub fn with_starter(config: Config, starter: Starter) -> Self {
        Openai { config, starter }
    }

    /// One POST to the chat completions of the endpoint.
    fn request(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        body: &str,
    ) -> Result<serde_json::Value, EngineFailure> {
        // Spec section 1: no text leaves the machine. ureq follows HTTP_PROXY
        // and 3xx by default, so both must stay off on this Agent.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.config.timeout))
            .proxy(None)
            .max_redirects(0)
            .build()
            .into();

        let mut request = agent
            .post(&endpoint.chat_url)
            .content_type("application/json");
        if !options.openai_api_key.is_empty() {
            request = request.header(
                "Authorization",
                &format!("Bearer {}", options.openai_api_key),
            );
        }

        let answer = request
            .send(body)
            .map_err(|error| self.classify(error, endpoint))?;

        let text = answer.into_body().read_to_string().map_err(|error| {
            EngineFailure::Failed(format!("The model server sent no body: {error}"))
        })?;

        serde_json::from_str(&text).map_err(|error| {
            EngineFailure::Failed(format!(
                "The model server sent an answer that is not a chat completion: {error}"
            ))
        })
    }

    fn classify(&self, error: ureq::Error, endpoint: &Endpoint) -> EngineFailure {
        let address = endpoint.address();
        match error {
            ureq::Error::Timeout(_) => EngineFailure::Timeout(format!(
                "The model did not answer within {} s on {address}",
                self.config.timeout.as_secs()
            )),
            ureq::Error::Io(inner) if is_unreachable(inner.kind()) => {
                EngineFailure::Unavailable(format!("No model server answered on {address}"))
            }
            ureq::Error::ConnectionFailed => {
                EngineFailure::Unavailable(format!("No model server answered on {address}"))
            }
            // llama.cpp binds the port before the weights are loaded and
            // answers 503 until they are, so this is a server still starting.
            ureq::Error::StatusCode(503) => EngineFailure::Unavailable(format!(
                "The model server is still loading on {address}"
            )),
            ureq::Error::StatusCode(status) => EngineFailure::Failed(format!(
                "The model server answered with HTTP {status} on {address}"
            )),
            other => {
                EngineFailure::Failed(format!("The model server could not be reached: {other}"))
            }
        }
    }

    /// Start the unit and wait until the server answers the Check.
    fn start_and_retry(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        body: &str,
    ) -> Result<serde_json::Value, EngineFailure> {
        if let Err(StartFailure(message)) = (self.starter)(&options.openai_model, endpoint) {
            return Err(EngineFailure::Unavailable(message));
        }

        let deadline = Instant::now() + self.config.startup_budget;
        loop {
            match self.request(endpoint, options, body) {
                Err(EngineFailure::Unavailable(message)) => {
                    if Instant::now() >= deadline {
                        return Err(EngineFailure::Unavailable(message));
                    }
                    sleep(PROBE_INTERVAL);
                }
                outcome => return outcome,
            }
        }
    }
}

impl Engine for Openai {
    fn slug(&self) -> &'static str {
        "openai"
    }

    fn check(&self, text: &str, options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure> {
        self.answer(text, options).map(|answer| answer.issues)
    }

    fn answer(&self, text: &str, options: &CheckOptions) -> Result<Answer, EngineFailure> {
        // Before anything is sent anywhere: the host must be this machine.
        let endpoint =
            endpoint::parse(&options.openai_base_url).map_err(EngineFailure::BadArguments)?;

        let body = prompt::request_body(text, options, force_of(options)).to_string();

        let answer = match self.request(&endpoint, options, &body) {
            Err(EngineFailure::Unavailable(message)) => {
                if self.config.start_unit {
                    self.start_and_retry(&endpoint, options, &body)?
                } else {
                    return Err(EngineFailure::Unavailable(message));
                }
            }
            outcome => outcome?,
        };

        let usage = Usage::from_response(&answer);
        let completion: ChatResponse = serde_json::from_value(answer).map_err(|error| {
            EngineFailure::Failed(format!(
                "The model server sent an answer that is not a chat completion: {error}"
            ))
        })?;
        let issues = response::issues_from(text, &completion).map_err(EngineFailure::Failed)?;
        Ok(Answer {
            issues,
            cost: None,
            usage,
        })
    }
}

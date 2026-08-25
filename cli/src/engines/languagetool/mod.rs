//! The default engine: the LanguageTool HTTP server on `127.0.0.1:8081`.
//!
//! Spec section 4. The adapter answers a Check by posting the text to
//! `/v2/check`. When nothing answers on the port, it starts the transient user
//! unit `grammachy-languagetool` and waits for the server to come up, so the
//! first Check after a login pays the start cost and every later Check reuses
//! the running unit.

pub mod response;
pub mod unit;

use std::io::ErrorKind;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::args::{CheckOptions, NativeLanguage};
use crate::engine::{Engine, EngineFailure};
use crate::envelope::Issue;

use response::CheckResponse;

/// The address spec section 4 fixes for the unit.
pub const DEFAULT_ADDRESS: &str = "127.0.0.1:8081";

/// The Check timeout of spec section 4.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long the adapter waits for a freshly started server to answer.
///
/// The unit starts a JVM and loads the `en-US` dictionaries, which takes
/// seconds on a cold page cache. This budget is separate from the Check
/// timeout, which applies to one request.
pub const DEFAULT_STARTUP_BUDGET: Duration = Duration::from_secs(30);

/// Time between two probes while the unit starts.
const PROBE_INTERVAL: Duration = Duration::from_millis(250);

/// What the adapter talks to and how long it waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Host and port, without a scheme, as it appears in the error message.
    pub address: String,
    /// Timeout of one request.
    pub timeout: Duration,
    /// Whether an unanswered port makes the adapter start the unit.
    pub start_unit: bool,
    /// How long to wait for a started unit to answer.
    pub startup_budget: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            address: DEFAULT_ADDRESS.to_string(),
            timeout: DEFAULT_TIMEOUT,
            start_unit: true,
            startup_budget: DEFAULT_STARTUP_BUDGET,
        }
    }
}

impl Config {
    /// Apply the test seams.
    ///
    /// `GRAMMACHY_LANGUAGETOOL_ADDRESS` points the adapter at another server
    /// and `GRAMMACHY_LANGUAGETOOL_START=never` keeps it from starting a unit.
    /// Both exist so the test suite and CI never touch a real systemd unit.
    /// Neither is a user-facing setting; settings live in `shell.json`
    /// (spec section 7).
    pub fn from_env() -> Self {
        let mut config = Config::default();
        if let Some(address) = std::env::var_os("GRAMMACHY_LANGUAGETOOL_ADDRESS") {
            let address = address.to_string_lossy().trim().to_string();
            if !address.is_empty() {
                config.address = address;
            }
        }
        if std::env::var_os("GRAMMACHY_LANGUAGETOOL_START").is_some_and(|value| value == "never") {
            config.start_unit = false;
        }
        config
    }

    fn port(&self) -> u16 {
        self.address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .unwrap_or(8081)
    }

    fn check_url(&self) -> String {
        format!("http://{}/v2/check", self.address)
    }
}

pub struct LanguageTool {
    config: Config,
}

impl LanguageTool {
    pub fn new(config: Config) -> Self {
        LanguageTool { config }
    }

    /// One POST to `/v2/check`.
    fn request(&self, body: &str) -> Result<CheckResponse, EngineFailure> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.config.timeout))
            .build()
            .into();

        let response = agent
            .post(self.config.check_url())
            .content_type("application/x-www-form-urlencoded")
            .send(body)
            .map_err(|error| self.classify(error))?;

        let text = response.into_body().read_to_string().map_err(|error| {
            EngineFailure::Failed(format!("LanguageTool sent no body: {error}"))
        })?;

        serde_json::from_str(&text).map_err(|error| {
            EngineFailure::Failed(format!(
                "LanguageTool sent an answer that is not the /v2/check JSON: {error}"
            ))
        })
    }

    fn classify(&self, error: ureq::Error) -> EngineFailure {
        let address = &self.config.address;
        match error {
            ureq::Error::Timeout(_) => EngineFailure::Timeout(format!(
                "LanguageTool did not answer within {} s on {address}",
                self.config.timeout.as_secs()
            )),
            ureq::Error::Io(inner) if is_unreachable(inner.kind()) => {
                EngineFailure::Unavailable(format!("LanguageTool did not answer on {address}"))
            }
            ureq::Error::ConnectionFailed => {
                EngineFailure::Unavailable(format!("LanguageTool did not answer on {address}"))
            }
            ureq::Error::StatusCode(status) => EngineFailure::Failed(format!(
                "LanguageTool answered with HTTP {status} on {address}"
            )),
            other => EngineFailure::Failed(format!("LanguageTool could not be reached: {other}")),
        }
    }

    /// Start the unit and wait until the server answers the Check.
    fn start_and_retry(&self, body: &str) -> Result<CheckResponse, EngineFailure> {
        if let Err(unit::StartFailure(message)) = unit::start(self.config.port()) {
            return Err(EngineFailure::Unavailable(message));
        }

        let deadline = Instant::now() + self.config.startup_budget;
        loop {
            match self.request(body) {
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

/// Whether an I/O error means nothing is listening yet.
fn is_unreachable(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::AddrNotAvailable
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe
    )
}

impl Engine for LanguageTool {
    fn slug(&self) -> &'static str {
        "languagetool"
    }

    fn check(&self, text: &str, options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure> {
        let body = request_body(text, options);

        let response = match self.request(&body) {
            Err(EngineFailure::Unavailable(message)) => {
                if self.config.start_unit {
                    self.start_and_retry(&body)?
                } else {
                    return Err(EngineFailure::Unavailable(message));
                }
            }
            outcome => outcome?,
        };

        Ok(response::issues_from(text, &response))
    }
}

/// The form body of one `/v2/check` request.
pub fn request_body(text: &str, options: &CheckOptions) -> String {
    let mut body = format!(
        "language={}&text={}",
        form_encode(options.target.as_str()),
        form_encode(text)
    );
    if let Some(mother_tongue) = mother_tongue(options.native) {
        body.push_str(&format!("&motherTongue={}", form_encode(mother_tongue)));
    }
    body
}

/// The `motherTongue` of spec section 4, or nothing when there is none.
pub fn mother_tongue(native: NativeLanguage) -> Option<&'static str> {
    match native {
        NativeLanguage::None | NativeLanguage::Ms => None,
        NativeLanguage::Zh => Some("zh-CN"),
        NativeLanguage::Ja => Some("ja-JP"),
        NativeLanguage::Es => Some("es"),
        NativeLanguage::Fr => Some("fr"),
        NativeLanguage::De => Some("de"),
        NativeLanguage::Pt => Some("pt"),
    }
}

/// Percent-encode one form field. Everything outside the unreserved set is
/// escaped, which is valid `application/x-www-form-urlencoded` and keeps the
/// text on the wire byte for byte.
fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{EngineSlug, TargetEnglish};

    fn options(native: NativeLanguage) -> CheckOptions {
        CheckOptions {
            native,
            target: TargetEnglish::EnUs,
            engine: EngineSlug::Languagetool,
        }
    }

    #[test]
    fn the_body_carries_the_language_and_the_text() {
        let body = request_body("He go home.", &options(NativeLanguage::None));

        assert_eq!(body, "language=en-US&text=He%20go%20home.");
    }

    #[test]
    fn the_native_language_maps_to_the_mother_tongue() {
        assert!(request_body("x", &options(NativeLanguage::Zh)).ends_with("&motherTongue=zh-CN"));
        assert!(request_body("x", &options(NativeLanguage::Ja)).ends_with("&motherTongue=ja-JP"));
        assert!(request_body("x", &options(NativeLanguage::Pt)).ends_with("&motherTongue=pt"));
        // `none` and `ms` send nothing.
        assert!(!request_body("x", &options(NativeLanguage::Ms)).contains("motherTongue"));
        assert!(!request_body("x", &options(NativeLanguage::None)).contains("motherTongue"));
    }

    #[test]
    fn an_astral_character_survives_the_encoding() {
        let body = request_body("\u{1F600}", &options(NativeLanguage::None));

        assert!(body.ends_with("text=%F0%9F%98%80"));
    }

    #[test]
    fn the_port_comes_from_the_address() {
        let config = Config {
            address: "127.0.0.1:9999".to_string(),
            ..Config::default()
        };

        assert_eq!(config.port(), 9999);
        assert_eq!(config.check_url(), "http://127.0.0.1:9999/v2/check");
    }
}

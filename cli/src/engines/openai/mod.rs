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
//!
//! llama-server ignores the `model` field of the request, so the weights it
//! already holds are the weights every Check gets. The adapter therefore reads
//! what the server serves before its first Check and never measures or checks
//! against another model (HUF-236). [`served`] holds that rule.
//!
//! A named mismatch reloads the unit, and a port that no reload can free is
//! [`mismatch`], the one refusal of that guard. It names both models, because
//! only a person can settle which of the two is wrong.

pub mod endpoint;
pub mod prompt;
pub mod response;
pub mod served;
pub mod unit;

use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::args::CheckOptions;
use crate::engine::{Answer, Engine, EngineFailure, Usage};
use crate::engines::local::{is_unreachable, StartFailure, Started};
use crate::envelope::Issue;
use crate::model::{self, Stopper};

use endpoint::Endpoint;
use response::ChatResponse;
use served::Served;

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

/// How long one served-model probe may take.
///
/// The probe runs on the loopback interface and reads a few hundred bytes, so
/// a server that is up answers it at once. The bound keeps a server that
/// accepts a connection and then says nothing from spending the Check timeout
/// before the Check has even started.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the adapter waits for a reloaded server to drop the wrong weights.
///
/// `systemctl --user stop` frees the port in well under a second. A port that
/// still serves the wrong model after this is a server the adapter did not
/// start, which no stop of the unit can reload.
const RELOAD_BUDGET: Duration = Duration::from_secs(10);

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
pub type Starter = Box<dyn Fn(&str, &Endpoint) -> Result<Started, StartFailure> + Send + Sync>;

/// What answers where the `grammachy-llama` unit listens.
///
/// The real one is [`unit::served_address`]. The guard reads it before it stops
/// anything, so a disagreement about weights on an Ollama port never takes down
/// a llama.cpp the run was not asked about.
pub type UnitAddress = Box<dyn Fn() -> Option<String> + Send + Sync>;

/// How many times one Check may bring a server up before it gives up.
///
/// A start whose server holds another model reloads once and starts once more.
/// A second round that still holds the wrong weights is a port this adapter
/// cannot settle, so the Check ends rather than loops.
const MAX_START_ROUNDS: usize = 2;

/// What the guard does with a server that named another model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mismatch {
    /// Stop the unit, so the start path can load the requested weights.
    Reload,
    /// Refuse the Check, because this adapter started that very server and a
    /// reload would load the same weights again.
    Refuse,
}

/// Why the guard did not reload a port that serves the wrong weights.
///
/// Each one earns its own remedy, because the advice a person can act on
/// differs. Naming a repair that already ran, or one that cannot run, wastes
/// the only sentence the refusal has.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Unreloadable {
    /// A start is forbidden here, so a stop would leave the port with nothing.
    StartForbidden,
    /// The unit does not serve this address, so no stop of it would free it.
    NotTheUnit,
    /// The stop did not run, and this is what it said.
    StopFailed(String),
    /// The stop ran, and the port still serves the wrong weights.
    StillServing,
    /// This adapter started that unit itself, so it holds what a start loads.
    FreshStart,
}

impl Unreloadable {
    /// The sentence that follows the two model names.
    fn remedy(&self, requested: &str) -> String {
        let elsewhere = format!("point openaiBaseUrl at a server that holds {requested}");
        match self {
            Unreloadable::StartForbidden => {
                format!("No start is allowed here, so no reload ran. Stop that server, or {elsewhere}.")
            }
            Unreloadable::NotTheUnit => format!(
                "The {} unit does not serve this address, so no reload ran. Stop that server, or {elsewhere}.",
                unit::UNIT_NAME,
            ),
            Unreloadable::StopFailed(why) => format!(
                "The reload did not run: {}. Stop that server, or {elsewhere}.",
                why.trim().trim_end_matches('.'),
            ),
            Unreloadable::StillServing => format!(
                "The {} unit stopped and the port still serves the other model. Stop that server, or {elsewhere}.",
                unit::UNIT_NAME,
            ),
            Unreloadable::FreshStart => format!(
                "This adapter started the {} unit itself, so another reload would load the same weights. Check openaiModel, or {elsewhere}.",
                unit::UNIT_NAME,
            ),
        }
    }
}

/// What one run of the served-model guard decided.
///
/// `weights` is what the server named, and `settled` is whether a later probe
/// may still change the answer. A silent port and a server that named no model
/// both leave the question open, so neither one settles it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Confirmed {
    weights: Option<String>,
    settled: bool,
    /// Whether a reload ran. A reload drops the weights any answer already in
    /// hand was measured against, so that answer goes away with them.
    reloaded: bool,
}

impl Confirmed {
    /// The server did not say which weights it holds, so the question is open.
    fn open() -> Self {
        Confirmed {
            weights: None,
            settled: false,
            reloaded: false,
        }
    }

    /// The server named the weights the Check asked for.
    fn named(weights: String) -> Self {
        Confirmed {
            weights: Some(weights),
            settled: true,
            reloaded: false,
        }
    }

    /// The same answer, reached after a reload dropped the old weights.
    fn after_reload(mut self) -> Self {
        self.reloaded = true;
        self
    }
}

/// What the served-model guard found so far.
///
/// `verdict` is `None` until the first probe. One adapter is built per bench
/// row and per `check` run, so the first probe is once per row and once per
/// Check of the shell. A settled verdict is never asked again, which is what
/// keeps a 365-item row at a small constant number of probes.
#[derive(Default)]
struct Guard {
    verdict: Option<Result<Option<String>, EngineFailure>>,
    settled: bool,
}

pub struct Openai {
    config: Config,
    starter: Starter,
    stopper: Stopper,
    unit_address: UnitAddress,
    served: Mutex<Guard>,
}

impl Openai {
    pub fn new(config: Config) -> Self {
        Openai {
            config,
            starter: Box::new(|model, endpoint| {
                unit::start(model, endpoint.bind_host(), endpoint.port)
            }),
            stopper: model::stopper(),
            unit_address: Box::new(unit::served_address),
            served: Mutex::default(),
        }
    }

    /// The adapter with another way to start the server, and no way to stop
    /// one.
    ///
    /// [`Self::with_server_control`] is the only route that hands the adapter a
    /// stopper, so a test that takes this one cannot reach systemd. A case that
    /// gives such a stub a served model then refuses a mismatch rather than
    /// stopping the developer's own unit.
    pub fn with_starter(config: Config, starter: Starter) -> Self {
        Openai {
            config,
            starter,
            stopper: Box::new(|unit: &str| {
                Err(format!("this adapter holds no way to stop {unit}"))
            }),
            unit_address: Box::new(|| None),
            served: Mutex::default(),
        }
    }

    /// The adapter with its own way to start and to stop the server.
    ///
    /// The stopper is what a reload runs, so the served-model guard is covered
    /// with no systemd at all.
    pub fn with_server_control(
        config: Config,
        starter: Starter,
        stopper: Stopper,
        unit_address: UnitAddress,
    ) -> Self {
        Openai {
            config,
            starter,
            stopper,
            unit_address,
            served: Mutex::default(),
        }
    }

    /// The weights this adapter confirmed the server holds, when it named them.
    ///
    /// `None` until the first Check, and `None` for a server that names no
    /// model. A benchmark row prints this beside the name it asked for, so the
    /// file says what was measured rather than only what was requested. A row
    /// whose port was silent until the start path brought a server up reports
    /// what that server named, because the guard asks it again once it answers.
    ///
    /// The answer is the weights file name and never the path a server gave,
    /// because one bench run is the whole committed benchmark file and a
    /// llama.cpp `--model` path holds the home directory it runs under.
    pub fn served_weights(&self) -> Option<String> {
        self.guard()
            .verdict
            .as_ref()
            .and_then(|verdict| verdict.as_ref().ok())
            .cloned()
            .flatten()
            .map(|served| served::file_name(&served).to_string())
    }

    /// The guard state, whatever an earlier panic left behind.
    ///
    /// A poisoned lock says a Check panicked, and the worst a stale verdict
    /// costs is one more probe. That is a better answer than a panic of its own
    /// inside an engine adapter.
    fn guard(&self) -> std::sync::MutexGuard<'_, Guard> {
        self.served
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    /// One GET to a route of the endpoint, as JSON.
    ///
    /// Every outcome but a parsed body is [`Served::Unknown`] or
    /// [`Served::Silent`]: this call only ever asks a question, so nothing it
    /// fails at may end a Check.
    fn probe_route(&self, url: &str, options: &CheckOptions) -> Served {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(PROBE_TIMEOUT.min(self.config.timeout)))
            .proxy(None)
            .max_redirects(0)
            .build()
            .into();

        let mut request = agent.get(url);
        if !options.openai_api_key.is_empty() {
            request = request.header(
                "Authorization",
                &format!("Bearer {}", options.openai_api_key),
            );
        }

        let answer = match request.call() {
            Ok(answer) => answer,
            Err(ureq::Error::Io(inner)) if is_unreachable(inner.kind()) => return Served::Silent,
            Err(ureq::Error::ConnectionFailed) => return Served::Silent,
            Err(_) => return Served::Unknown,
        };
        let Ok(text) = answer.into_body().read_to_string() else {
            return Served::Unknown;
        };
        let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Served::Unknown;
        };
        match served::from_models(&raw, &options.openai_model).or_else(|| served::from_props(&raw))
        {
            Some(id) => Served::Id(id),
            None => Served::Unknown,
        }
    }

    /// Ask the server what it serves, over both routes it may answer on.
    ///
    /// An answer that matches the requested model wins, whichever route gave
    /// it. `llama-server --alias` renames what `/v1/models` reports and leaves
    /// `/props` naming the weights file, so a route that merely names something
    /// must never shadow one that names the right thing. That is the rule
    /// [`served::from_models`] applies inside one list, across the two routes.
    /// Only the first named answer is kept for the refusal.
    fn probe(&self, endpoint: &Endpoint, options: &CheckOptions) -> Served {
        let mut named: Option<String> = None;
        let mut found = Served::Silent;
        for url in [&endpoint.models_url, &endpoint.props_url] {
            match self.probe_route(url, options) {
                Served::Id(id) if served::matches(&id, &options.openai_model) => {
                    return Served::Id(id)
                }
                Served::Id(id) => {
                    named.get_or_insert(id);
                }
                Served::Unknown => found = Served::Unknown,
                Served::Silent => {}
            }
        }
        match named {
            Some(id) => Served::Id(id),
            None => found,
        }
    }

    /// The served-model guard before the first Check of this adapter's life.
    ///
    /// A silent port and a server that names no model both leave the question
    /// open rather than refuse the Check: the start path brings a server up
    /// with `openaiModel` itself, and `openaiBaseUrl` may name any
    /// OpenAI-compatible server, of which only llama-server says what it holds.
    /// Neither answer settles anything, so [`Self::confirm_started`] asks again
    /// once a server does answer.
    fn confirm(&self, endpoint: &Endpoint, options: &CheckOptions) -> Result<(), EngineFailure> {
        let mut guard = self.guard();
        if let Some(verdict) = guard.verdict.clone() {
            return verdict.map(drop);
        }
        self.store(&mut guard, endpoint, options, Mismatch::Reload)
            .map(drop)
    }

    /// The served-model guard after the start path brought a server up.
    ///
    /// A silent port and a port still reading its weights both answered the
    /// first probe with nothing final, so the answer in hand was measured
    /// against weights this adapter never confirmed. Asking again is what
    /// closes that window.
    ///
    /// What a named mismatch earns here follows `started`. `Started::Fresh`
    /// means systemd-run built the unit from `openaiModel` itself, so another
    /// reload would load the same weights and the Check refuses. A unit that
    /// was already running is the HUF-236 case: the start call did nothing, the
    /// weights on the port belong to an earlier session, and one stop reloads
    /// them. So that one takes the ordinary verdict and reloads.
    ///
    /// The answer comes back as whether the Check in hand still stands. A
    /// reload dropped the weights it was measured against, so it does not.
    fn confirm_started(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        started: Started,
    ) -> Result<bool, EngineFailure> {
        let mut guard = self.guard();
        if guard.settled {
            return guard.verdict.clone().unwrap_or(Ok(None)).map(|_| true);
        }
        let on_mismatch = match started {
            Started::Fresh => Mismatch::Refuse,
            Started::AlreadyRunning => Mismatch::Reload,
        };
        self.store(&mut guard, endpoint, options, on_mismatch)
            .map(|confirmed| !confirmed.reloaded)
    }

    /// Probe the server and record what it answered.
    ///
    /// Only a settled verdict is final. An open one is kept for the report and
    /// asked again at the one boundary that can change it, so a server that
    /// names no model still costs one probe rather than one per sentence.
    fn store(
        &self,
        guard: &mut Guard,
        endpoint: &Endpoint,
        options: &CheckOptions,
        on_mismatch: Mismatch,
    ) -> Result<Confirmed, EngineFailure> {
        let outcome = self.verify_served(endpoint, options, on_mismatch);
        guard.settled = outcome.as_ref().is_ok_and(|it| it.settled) || outcome.is_err();
        guard.verdict = Some(
            outcome
                .as_ref()
                .map(|it| it.weights.clone())
                .map_err(Clone::clone),
        );
        outcome
    }

    /// One probe, and the verdict it earns.
    fn verify_served(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        on_mismatch: Mismatch,
    ) -> Result<Confirmed, EngineFailure> {
        match self.probe(endpoint, options) {
            Served::Id(id) if served::matches(&id, &options.openai_model) => {
                Ok(Confirmed::named(id))
            }
            Served::Id(id) if on_mismatch == Mismatch::Refuse => Err(mismatch(
                endpoint,
                &id,
                &options.openai_model,
                Unreloadable::FreshStart,
            )),
            Served::Id(id) => self.reload(endpoint, options, &id),
            Served::Silent | Served::Unknown => Ok(Confirmed::open()),
        }
    }

    /// Drop a server that holds the wrong weights, or say why it cannot be
    /// dropped.
    ///
    /// The reload is the stop alone. The start path already knows how to bring
    /// the server up for `openaiModel`, so this only has to free the port and
    /// wait for that to take.
    ///
    /// The stop only runs when the unit is the server behind `endpoint`.
    /// `openaiBaseUrl` accepts any loopback server, so it may name an Ollama, an
    /// LM Studio, or a hand-run llama-server on another port. Stopping the unit
    /// then takes down a server no part of this disagreement is about, and one
    /// that another run may be measuring on. So the address decides, and every
    /// other case refuses without a stop and without the reload wait.
    ///
    /// Four things end a reload, and each is the one refusal of [`mismatch`]
    /// with a reason of its own: the start is forbidden, the unit does not
    /// serve this address, the stop did not run, or the port still holds the
    /// wrong weights after a stop that did. None of the four is a machine that
    /// is broken, and each leaves the same two model names for a person to
    /// settle.
    fn reload(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        was: &str,
    ) -> Result<Confirmed, EngineFailure> {
        let refuse = |why: Unreloadable| mismatch(endpoint, was, &options.openai_model, why);
        if !self.config.start_unit {
            return Err(refuse(Unreloadable::StartForbidden));
        }
        if !self.unit_serves(endpoint) {
            return Err(refuse(Unreloadable::NotTheUnit));
        }
        if let Err(why) = (self.stopper)(unit::UNIT_NAME) {
            return Err(refuse(Unreloadable::StopFailed(why)));
        }

        let deadline = Instant::now() + RELOAD_BUDGET;
        loop {
            match self.probe(endpoint, options) {
                Served::Id(id) if served::matches(&id, &options.openai_model) => {
                    return Ok(Confirmed::named(id).after_reload())
                }
                // The port is free, so the start path loads the right weights.
                // What it loads is still unconfirmed, so the question stays
                // open until that server answers.
                Served::Silent => return Ok(Confirmed::open().after_reload()),
                _ if Instant::now() >= deadline => return Err(refuse(Unreloadable::StillServing)),
                _ => sleep(PROBE_INTERVAL),
            }
        }
    }

    /// Whether the `grammachy-llama` unit is the server behind one endpoint.
    ///
    /// [`unit::start`] writes the host and the port into the unit command, so
    /// the unit says where it listens. An address it does not name belongs to a
    /// server this adapter never started and must never stop.
    fn unit_serves(&self, endpoint: &Endpoint) -> bool {
        (self.unit_address)()
            .is_some_and(|address| address == format!("{}:{}", endpoint.bind_host(), endpoint.port))
    }

    /// Start the unit and wait until the server answers the Check.
    fn start_and_retry(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        body: &str,
    ) -> Result<(serde_json::Value, Started), EngineFailure> {
        let started = match (self.starter)(&options.openai_model, endpoint) {
            Ok(started) => started,
            Err(StartFailure(message)) => return Err(EngineFailure::Unavailable(message)),
        };

        let deadline = Instant::now() + self.config.startup_budget;
        loop {
            match self.request(endpoint, options, body) {
                Err(EngineFailure::Unavailable(message)) => {
                    if Instant::now() >= deadline {
                        return Err(EngineFailure::Unavailable(message));
                    }
                    sleep(PROBE_INTERVAL);
                }
                outcome => return outcome.map(|raw| (raw, started)),
            }
        }
    }

    /// One Check, sent only to a server whose weights the guard confirmed.
    ///
    /// A silent port and a port still reading its weights settle nothing, so
    /// the guard asks again once a server answers. A reload at that point drops
    /// the weights the answer in hand came from, so that answer goes away and
    /// the Check runs once more against what the start path loads next.
    fn checked(
        &self,
        endpoint: &Endpoint,
        options: &CheckOptions,
        body: &str,
    ) -> Result<serde_json::Value, EngineFailure> {
        for _ in 0..MAX_START_ROUNDS {
            // A settled guard answers from its own record and sends nothing.
            self.confirm(endpoint, options)?;

            let message = match self.request(endpoint, options, body) {
                Err(EngineFailure::Unavailable(message)) => message,
                outcome => return outcome,
            };
            if !self.config.start_unit {
                return Err(EngineFailure::Unavailable(message));
            }

            let (raw, started) = self.start_and_retry(endpoint, options, body)?;
            if self.confirm_started(endpoint, options, started)? {
                return Ok(raw);
            }
        }
        Err(EngineFailure::Unavailable(format!(
            "The model server on {} did not settle on one model after {MAX_START_ROUNDS} reloads",
            endpoint.address(),
        )))
    }
}

/// The one refusal of the served-model guard, naming both models.
///
/// It is `bad_arguments` rather than an engine error, because nothing about
/// this machine is broken: the base URL and the model setting disagree, and
/// only a person can settle which of the two is wrong. Every port the guard
/// cannot reload ends here, and `why` picks the one remedy that fits it.
///
/// The served model is named by its weights file alone. A llama.cpp path holds
/// the home directory it runs under, and this message reaches a log and a bench
/// report.
fn mismatch(
    endpoint: &Endpoint,
    served: &str,
    requested: &str,
    why: Unreloadable,
) -> EngineFailure {
    EngineFailure::BadArguments(format!(
        "The model server on {} serves {}, and this Check asks for {requested}. {}",
        endpoint.address(),
        served::file_name(served),
        why.remedy(requested),
    ))
}

impl Engine for Openai {
    fn slug(&self) -> &'static str {
        "openai"
    }

    fn served_model(&self) -> Option<String> {
        self.served_weights()
    }

    fn check(&self, text: &str, options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure> {
        self.answer(text, options).map(|answer| answer.issues)
    }

    fn answer(&self, text: &str, options: &CheckOptions) -> Result<Answer, EngineFailure> {
        // Before anything is sent anywhere: the host must be this machine.
        let endpoint =
            endpoint::parse(&options.openai_base_url).map_err(EngineFailure::BadArguments)?;

        let body = prompt::request_body(text, options, force_of(options)).to_string();

        // Before the first Check: the server must hold the weights that were
        // asked for. Nothing is measured or checked against another model.
        let answer = self.checked(&endpoint, options, &body)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> Endpoint {
        endpoint::parse("http://127.0.0.1:8080").expect("a loopback URL parses")
    }

    /// Every refusal names both models, and each reason gives advice that fits
    /// it. A remedy that names a repair which already ran, or one that cannot
    /// run, wastes the only sentence the refusal has.
    #[test]
    fn each_refusal_names_both_models_and_advises_what_is_left() {
        let reasons = [
            Unreloadable::StartForbidden,
            Unreloadable::NotTheUnit,
            Unreloadable::StopFailed("Unit grammachy-llama.service not loaded".to_string()),
            Unreloadable::StillServing,
            Unreloadable::FreshStart,
        ];

        for why in reasons {
            let EngineFailure::BadArguments(message) = mismatch(
                &endpoint(),
                "/models/granite-4.2-3b-Q4_K_M.gguf",
                "gemma-4-e4b-it",
                why.clone(),
            ) else {
                panic!("the guard refuses with bad_arguments");
            };

            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains("gemma-4-e4b-it"), "{message}");
            assert!(
                !message.contains("/models/"),
                "the served weights are named by file: {message}"
            );
        }

        // The stop already ran in these two, so neither may ask for it again.
        for why in [
            Unreloadable::StillServing,
            Unreloadable::StopFailed("x".to_string()),
        ] {
            let remedy = why.remedy("gemma-4-e4b-it");
            assert!(
                !remedy.contains(&format!("Stop the {}", unit::UNIT_NAME)),
                "{remedy}"
            );
        }
        // These two ran no stop, so neither may report one.
        for why in [Unreloadable::StartForbidden, Unreloadable::NotTheUnit] {
            let remedy = why.remedy("gemma-4-e4b-it");
            assert!(remedy.contains("no reload ran"), "{remedy}");
        }
    }
}

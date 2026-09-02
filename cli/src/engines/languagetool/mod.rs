//! The opt-in engine: the LanguageTool HTTP server in a transient user unit.
//!
//! Spec section 4. The adapter answers a Check by posting the text to
//! `/v2/check` on the loopback listener the unit `grammachy-languagetool`
//! owns. It never trusts a port number: [`crate::engines::listener`] reads
//! which socket the unit's own processes hold before every request, so a
//! Selection can only reach a server this plugin started. When the unit is
//! not running, the adapter starts it on a free loopback port and waits for
//! its listener, so the first Check after a login pays the start cost and
//! every later Check reuses the running unit.

pub mod response;
pub mod unit;

use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread::sleep;
use std::time::{Duration, Instant};

use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport,
};
use ureq::Agent;

use crate::args::{CheckOptions, NativeLanguage};
use crate::engine::{Engine, EngineFailure};
use crate::engines::listener::{self, Owned, Peer};
use crate::engines::local::{self, is_unreachable};
use crate::envelope::Issue;

use response::CheckResponse;

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

/// How long one loopback connect may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// The most bytes of one `/v2/check` answer the adapter reads.
///
/// A Check is at most 5,000 UTF-16 units and a match is a few hundred bytes,
/// so a real answer is well under 1 MiB. Anything past this is not an answer
/// to the question that was asked, and it is refused before it is parsed.
pub const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// Where the adapter sends a Check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// The loopback listener this transient unit owns, read before every
    /// request. The product path.
    Unit(String),
    /// One fixed loopback address, which no unit stands behind. The test seam:
    /// the stub servers of the test suite are not units. The adapter never
    /// starts a unit for it.
    Fixed(SocketAddr),
}

impl Endpoint {
    /// A fixed endpoint, refused unless it is on the loopback interface.
    pub fn fixed(address: &str) -> Result<Endpoint, String> {
        let address: SocketAddr = address
            .trim()
            .parse()
            .map_err(|error| format!("{address:?} is not a socket address: {error}"))?;
        if !address.ip().is_loopback() {
            return Err(format!(
                "{address} is not a loopback address, and a Selection never leaves the machine."
            ));
        }
        Ok(Endpoint::Fixed(address))
    }
}

/// Where one request goes, once the endpoint is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// The stub server of the test seam, with no process to prove.
    Fixed(SocketAddr),
    /// The unit's listener, whose main process must accept the connection.
    Unit(Peer),
}

impl Target {
    fn address(&self) -> SocketAddr {
        match self {
            Target::Fixed(address) => *address,
            Target::Unit(peer) => peer.address,
        }
    }
}

/// The connector of the product path: it connects to the listener the unit
/// owns and refuses the connection unless the unit's main process accepted
/// it. The proof is on the socket the request then uses, so nothing that
/// takes the port between the listener read and the request can receive
/// the text.
#[derive(Debug)]
struct BoundConnector {
    peer: Peer,
}

impl Connector<()> for BoundConnector {
    type Out = BoundTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        _chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let stream = TcpStream::connect_timeout(&self.peer.address, CONNECT_TIMEOUT)
            .map_err(ureq::Error::Io)?;
        stream.set_nodelay(true).map_err(ureq::Error::Io)?;
        listener::accepted_by(&stream, self.peer.pid)
            .map_err(|why| ureq::Error::Io(io::Error::other(why)))?;

        let config = &details.config;
        let buffers = LazyBuffers::new(config.input_buffer_size(), config.output_buffer_size());
        Ok(Some(BoundTransport { stream, buffers }))
    }
}

/// The proven connection, driven the way ureq drives a plain TCP one. ureq
/// does not export its own TCP transport, so this is the same loop over the
/// stream the connector proved.
#[derive(Debug)]
struct BoundTransport {
    stream: TcpStream,
    buffers: LazyBuffers,
}

impl Transport for BoundTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream
            .set_write_timeout(timeout.not_zero().map(|after| *after))?;
        let output = &self.buffers.output()[..amount];
        match self.stream.write_all(output) {
            Ok(()) => Ok(()),
            Err(error) if is_timeout(&error) => Err(ureq::Error::Timeout(timeout.reason)),
            Err(error) => Err(error.into()),
        }
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream
            .set_read_timeout(timeout.not_zero().map(|after| *after))?;
        let input = self.buffers.input_append_buf();
        let amount = match self.stream.read(input) {
            Ok(amount) => amount,
            Err(error) if is_timeout(&error) => return Err(ureq::Error::Timeout(timeout.reason)),
            Err(error) => return Err(error.into()),
        };
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    /// One request per agent, so the connection is never pooled or reused.
    fn is_open(&mut self) -> bool {
        false
    }
}

/// A read or write that ran out of its socket timeout.
fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)
}

/// What the adapter talks to and how long it waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub endpoint: Endpoint,
    /// Timeout of one request.
    pub timeout: Duration,
    /// Whether an inactive unit makes the adapter start it.
    pub start_unit: bool,
    /// How long to wait for a started unit to open its listener.
    pub startup_budget: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            endpoint: Endpoint::Unit(unit::UNIT_NAME.to_string()),
            timeout: DEFAULT_TIMEOUT,
            start_unit: true,
            startup_budget: DEFAULT_STARTUP_BUDGET,
        }
    }
}

impl Config {
    /// Apply the test seams, in a debug build only.
    ///
    /// `GRAMMACHY_LANGUAGETOOL_ADDRESS` points the adapter at a fixed loopback
    /// address and `GRAMMACHY_LANGUAGETOOL_START=never` keeps it from starting
    /// a unit. Both exist so the test suite and CI never touch a real systemd
    /// unit. The shipped binary is a release build and reads neither, so no
    /// environment can point a Selection anywhere but the unit's own listener.
    /// Neither is a user-facing setting; settings live in `shell.json`
    /// (spec section 7).
    pub fn from_env() -> Self {
        let mut config = Config::default();
        if !cfg!(debug_assertions) {
            return config;
        }
        if let Some(address) = std::env::var_os("GRAMMACHY_LANGUAGETOOL_ADDRESS") {
            let address = address.to_string_lossy();
            if !address.trim().is_empty() {
                match Endpoint::fixed(&address) {
                    Ok(endpoint) => config.endpoint = endpoint,
                    Err(why) => eprintln!("GRAMMACHY_LANGUAGETOOL_ADDRESS is ignored: {why}"),
                }
            }
        }
        if std::env::var_os("GRAMMACHY_LANGUAGETOOL_START").is_some_and(|value| value == "never") {
            config.start_unit = false;
        }
        config
    }
}

pub struct LanguageTool {
    config: Config,
}

impl LanguageTool {
    pub fn new(config: Config) -> Self {
        LanguageTool { config }
    }

    /// One POST to `/v2/check` on one target.
    ///
    /// The answer is read up to [`MAX_RESPONSE_BYTES`] and refused past that,
    /// before any of it is parsed.
    fn request(&self, target: &Target, body: &str) -> Result<CheckResponse, EngineFailure> {
        let config = Agent::config_builder()
            .timeout_global(Some(self.config.timeout))
            .build();
        let agent = match target {
            Target::Fixed(_) => Agent::new_with_config(config),
            Target::Unit(peer) => Agent::with_parts(
                config,
                BoundConnector { peer: peer.clone() },
                DefaultResolver::default(),
            ),
        };
        let address = target.address();

        let response = agent
            .post(format!("http://{address}/v2/check"))
            .content_type("application/x-www-form-urlencoded")
            .send(body)
            .map_err(|error| self.classify(address, error))?;

        let text = response
            .into_body()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(|error| match error {
                ureq::Error::BodyExceedsLimit(limit) => EngineFailure::Failed(format!(
                    "LanguageTool sent more than {limit} bytes, which no Check answer needs"
                )),
                other => EngineFailure::Failed(format!("LanguageTool sent no body: {other}")),
            })?;

        serde_json::from_str(&text).map_err(|error| {
            EngineFailure::Failed(format!(
                "LanguageTool sent an answer that is not the /v2/check JSON: {error}"
            ))
        })
    }

    fn classify(&self, address: SocketAddr, error: ureq::Error) -> EngineFailure {
        match error {
            ureq::Error::Timeout(_) => EngineFailure::Timeout(format!(
                "LanguageTool did not answer within {} s on {address}",
                self.config.timeout.as_secs()
            )),
            ureq::Error::Io(inner) if is_unreachable(inner.kind()) => {
                EngineFailure::Unavailable(format!("LanguageTool did not answer on {address}"))
            }
            // The connector refused the peer, and its message says why.
            ureq::Error::Io(inner) if inner.kind() == ErrorKind::Other => {
                EngineFailure::Unavailable(format!("LanguageTool was not reached: {inner}"))
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

    /// Where this Check goes, or why there is nowhere.
    ///
    /// For the unit endpoint this is the listener the unit's main process
    /// holds right now, and the unit must be one this plugin started. An
    /// inactive unit is started first when the configuration allows it, and
    /// an active unit without a listener yet is waited for.
    fn resolve(&self) -> Result<Target, EngineFailure> {
        let unit = match &self.config.endpoint {
            Endpoint::Fixed(address) => return Ok(Target::Fixed(*address)),
            Endpoint::Unit(unit) => unit,
        };

        let peer = match listener::owned_listener(unit) {
            Owned::Listening(peer) => peer,
            Owned::Starting => self.wait_for_listener(unit)?,
            Owned::Unknown(why) => return Err(EngineFailure::Unavailable(why)),
            Owned::Inactive => {
                if !self.config.start_unit {
                    return Err(EngineFailure::Unavailable(format!(
                        "LanguageTool is not running: the unit {unit} is inactive"
                    )));
                }
                let port = local::free_loopback_port()
                    .map_err(|unit::StartFailure(message)| EngineFailure::Unavailable(message))?;
                unit::start(port)
                    .map_err(|unit::StartFailure(message)| EngineFailure::Unavailable(message))?;
                self.wait_for_listener(unit)?
            }
        };

        unit::launched_here(&peer).map_err(|why| {
            EngineFailure::Unavailable(format!(
                "The unit {unit} is not one Grammachy started: {why}. Stop it with: systemctl --user stop {unit}"
            ))
        })?;
        Ok(Target::Unit(peer))
    }

    /// Poll the unit until it owns a listener, within the startup budget.
    fn wait_for_listener(&self, unit: &str) -> Result<Peer, EngineFailure> {
        let deadline = Instant::now() + self.config.startup_budget;
        loop {
            match listener::owned_listener(unit) {
                Owned::Listening(peer) => return Ok(peer),
                Owned::Unknown(why) => return Err(EngineFailure::Unavailable(why)),
                Owned::Inactive => {
                    return Err(EngineFailure::Unavailable(format!(
                        "LanguageTool stopped before it answered: the unit {unit} is inactive"
                    )))
                }
                Owned::Starting => {}
            }
            if Instant::now() >= deadline {
                return Err(EngineFailure::Unavailable(format!(
                    "LanguageTool did not open a loopback listener within {} s: the unit {unit} is active without one",
                    self.config.startup_budget.as_secs()
                )));
            }
            sleep(PROBE_INTERVAL);
        }
    }
}

impl Engine for LanguageTool {
    fn slug(&self) -> &'static str {
        "languagetool"
    }

    fn check(&self, text: &str, options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure> {
        let body = request_body(text, options);
        let target = self.resolve()?;
        let response = self.request(&target, &body)?;
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
    use crate::engines::listener::ExecStart;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::mpsc;

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
    fn the_target_english_is_the_language_field() {
        let mut options = options(NativeLanguage::None);
        options.target = TargetEnglish::EnGb;

        assert!(request_body("Colour.", &options).starts_with("language=en-GB&"));
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
    fn a_fixed_endpoint_is_loopback_only() {
        assert_eq!(
            Endpoint::fixed("127.0.0.1:9999"),
            Ok(Endpoint::Fixed("127.0.0.1:9999".parse().unwrap()))
        );
        assert!(Endpoint::fixed("[::1]:9999").is_ok());

        let refused = Endpoint::fixed("10.0.0.5:8081").expect_err("not loopback");
        assert!(refused.contains("not a loopback address"), "{refused}");
        assert!(Endpoint::fixed("languagetool.org:443").is_err());
        assert!(Endpoint::fixed("").is_err());
    }

    #[test]
    fn the_product_endpoint_is_the_unit() {
        assert_eq!(
            Config::default().endpoint,
            Endpoint::Unit("grammachy-languagetool".to_string())
        );
    }

    /// One stub `/v2/check` server on a free loopback port, in this process.
    ///
    /// The proof of [`BoundConnector`] reads which process accepted the
    /// connection, so a listener this process owns is what makes the product
    /// path testable without a systemd unit. `answer` writes the response on
    /// the accepted socket, after the request and its body are read.
    fn stub_server<F>(answer: F) -> (SocketAddr, mpsc::Sender<()>, std::thread::JoinHandle<()>)
    where
        F: FnOnce(&mut TcpStream) + Send + 'static,
    {
        let server = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = server.local_addr().unwrap();
        let (release, released) = mpsc::channel::<()>();
        let serving = std::thread::spawn(move || {
            let (mut accepted, _) = server.accept().expect("the client connects");
            read_request(&accepted);
            answer(&mut accepted);
            // Hold the accepted end open until the proof and the read are done.
            let _ = released.recv_timeout(Duration::from_secs(10));
        });
        (address, release, serving)
    }

    /// The request head and its whole body, so the client never sees a reset.
    fn read_request(stream: &TcpStream) {
        let mut reader = io::BufReader::new(stream);
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }
        let mut body = vec![0u8; length];
        let _ = reader.read_exact(&mut body);
    }

    fn write_json(stream: &mut TcpStream, json: &str) {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            json.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(json.as_bytes());
        let _ = stream.flush();
    }

    fn peer_of(address: SocketAddr, pid: u32) -> Peer {
        Peer {
            address,
            pid,
            transient: true,
            exec_start: ExecStart::default(),
        }
    }

    /// The product path end to end: the connector proves the accepting
    /// process, the transport writes the request and reads the answer, and
    /// the body parses into the matches the Check reads.
    #[test]
    fn the_proven_connection_carries_one_check_answer() {
        let json = r#"{"matches":[{"offset":3,"length":2,"message":"He goes home.","replacements":[{"value":"goes"}],"rule":{"id":"HE_GO","issueType":"grammar"}}]}"#;
        let (address, release, serving) = stub_server(move |stream| write_json(stream, json));
        let engine = LanguageTool::new(Config::default());

        let answer = engine
            .request(
                &Target::Unit(peer_of(address, std::process::id())),
                "language=en-US&text=He%20go%20home.",
            )
            .expect("the stub answers the check");

        assert_eq!(answer.matches.len(), 1);
        assert_eq!(answer.matches[0].offset, 3);
        assert_eq!(answer.matches[0].length, 2);
        assert_eq!(answer.matches[0].replacements[0].value, "goes");

        let _ = release.send(());
        serving.join().unwrap();
    }

    /// A listener some other process accepted on is refused before the
    /// request is written, which is the whole point of the connector.
    #[test]
    fn a_connection_another_process_accepted_is_refused() {
        let (address, release, serving) = stub_server(move |stream| write_json(stream, "{}"));
        let engine = LanguageTool::new(Config::default());

        let failure = engine
            .request(&Target::Unit(peer_of(address, 1)), "language=en-US&text=x")
            .expect_err("init did not accept it");

        match failure {
            EngineFailure::Unavailable(why) => {
                assert!(why.contains("accepted by a process other than 1"), "{why}")
            }
            other => panic!("{other:?}"),
        }

        let _ = release.send(());
        serving.join().unwrap();
    }

    /// An answer past the cap is refused before it is parsed, so no producer
    /// on the loopback port can fill memory.
    #[test]
    fn an_answer_past_the_byte_cap_is_refused() {
        let (address, release, serving) = stub_server(move |stream| {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                MAX_RESPONSE_BYTES + 1
            );
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            let chunk = vec![b'0'; 64 * 1024];
            let mut written = 0u64;
            while written <= MAX_RESPONSE_BYTES {
                if stream.write_all(&chunk).is_err() {
                    return;
                }
                written += chunk.len() as u64;
            }
        });
        let engine = LanguageTool::new(Config::default());

        let failure = engine
            .request(
                &Target::Unit(peer_of(address, std::process::id())),
                "language=en-US&text=x",
            )
            .expect_err("the answer is past the cap");

        match failure {
            EngineFailure::Failed(why) => {
                assert!(why.contains(&MAX_RESPONSE_BYTES.to_string()), "{why}")
            }
            other => panic!("{other:?}"),
        }

        let _ = release.send(());
        serving.join().unwrap();
    }
}

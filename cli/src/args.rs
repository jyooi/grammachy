//! Command line surface, spec sections 5.1, 5.2, 10, and 13.1.

use clap::{Parser, Subcommand, ValueEnum};

use crate::settings::{self, StoredSettings};

#[derive(Debug, Parser)]
#[command(
    name = "grammachy",
    version,
    about = "Grammar and spelling checks for Omarchy"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the UTF-8 text on stdin and print one JSON envelope on stdout.
    Check(CheckArgs),

    /// Split the Draft on stdin into Chunks that each fit one Check.
    Chunk(ChunkArgs),

    /// Run the interference fixture through every engine this machine reaches
    /// and print the benchmark file on stdout (spec section 13.1).
    Bench(BenchArgs),

    /// Report what this machine still needs, one line per piece.
    Doctor(DoctorArgs),

    /// Install the hotkeys, the menu entry, and the weights, without a password.
    Setup(SetupArgs),

    /// See, fetch, and delete the Local LLM weights this machine keeps.
    Model(ModelArgs),
}

#[derive(Debug, Parser)]
pub struct ModelArgs {
    #[command(subcommand)]
    pub verb: ModelVerb,
}

#[derive(Debug, Subcommand)]
pub enum ModelVerb {
    /// One row per catalogue model, with what it has on disk.
    List,

    /// Fetch one catalogue model, resuming a part file it already has.
    Download(ModelNameArgs),

    /// Delete one catalogue model's weights file and its part file.
    Remove(ModelNameArgs),
}

#[derive(Debug, Parser)]
pub struct ModelNameArgs {
    /// The catalogue name, as `grammachy model list` prints it.
    pub name: String,
}

#[derive(Debug, Parser)]
pub struct ChunkArgs {
    /// The engine whose Check size limit the Chunks are packed to.
    ///
    /// Omitted uses the stored entry, then the default, the same order one
    /// Check resolves in (spec section 7).
    #[arg(long, value_enum)]
    pub engine: Option<EngineSlug>,
}

#[derive(Debug, Parser)]
pub struct BenchArgs {
    /// The engine the named models run on: openai (the default) or openrouter.
    ///
    /// This does not narrow the Engines table: one run prints the whole file.
    #[arg(long, value_enum)]
    pub engine: Option<EngineSlug>,

    /// A model to evaluate on --engine, repeatable, one Models row each.
    #[arg(long = "model", value_name = "NAME")]
    pub models: Vec<String>,

    /// A model to evaluate through openrouter, repeatable, one Models row
    /// each, so one run holds local and cloud rows side by side.
    #[arg(long = "cloud-model", value_name = "ID")]
    pub cloud_models: Vec<String>,

    /// A bound on what the whole run may spend on openrouter, in USD. The run
    /// weighs it between Checks and ends a cloud row before the next Check
    /// would pass it. Required when any row runs through openrouter, refused
    /// otherwise.
    #[arg(long = "max-cost", value_name = "USD")]
    pub max_cost: Option<f64>,

    /// Write every Check's answer to <DIR>/checks.json, the input of the judge.
    #[arg(long = "record", value_name = "DIR")]
    pub record: Option<std::path::PathBuf>,
}

#[derive(Debug, Parser)]
pub struct DoctorArgs {
    /// The engine the one-line diagnosis is about. Omitted uses the stored
    /// entry, then the default.
    #[arg(long, value_enum)]
    pub engine: Option<EngineSlug>,

    /// Print the report as one JSON envelope instead of as text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Parser)]
pub struct SetupArgs {
    /// Take the hotkeys, the menu entry, and the OpenRouter key out again,
    /// keeping the weights.
    #[arg(long)]
    pub remove: bool,

    /// Read one OpenRouter key from stdin and write it to the key file of
    /// spec section 4. It writes nothing else and it prints no key.
    #[arg(long = "openrouter-key", conflicts_with = "remove")]
    pub openrouter_key: bool,
}

#[derive(Debug, Parser)]
pub struct CheckArgs {
    /// The language the user thinks in. Omitted uses the stored entry, then none.
    #[arg(long, value_enum)]
    pub native: Option<NativeLanguage>,

    /// The English variant the text is checked against.
    #[arg(long, value_enum)]
    pub target: Option<TargetEnglish>,

    /// The engine that performs the Check.
    #[arg(long, value_enum)]
    pub engine: Option<EngineSlug>,

    /// Whether the local engine thinks before it answers. Omitted uses the
    /// stored `localThinking`, then the default `on`.
    #[arg(long, value_enum)]
    pub thinking: Option<Thinking>,

    /// The model id the `openrouter` engine asks for, such as
    /// `deepseek/deepseek-v4-flash`. Omitted uses the stored entry.
    #[arg(long = "openrouter-model", value_name = "ID")]
    pub openrouter_model: Option<String>,
}

/// The `--thinking` flag of spec section 4, which wins over the Setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Thinking {
    On,
    Off,
}

impl Thinking {
    pub fn is_on(self) -> bool {
        self == Thinking::On
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum NativeLanguage {
    None,
    Zh,
    Ms,
    Es,
    Fr,
    De,
    Pt,
    Ja,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TargetEnglish {
    #[value(name = "en-US")]
    EnUs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EngineSlug {
    Languagetool,
    Openai,
    Harper,
    Openrouter,
}

impl NativeLanguage {
    /// The stored value of `nativeLanguage`, or `None` for anything the spec
    /// does not list, which then reads as the default (spec section 7).
    pub fn from_stored(value: &str) -> Option<Self> {
        match value {
            "none" => Some(NativeLanguage::None),
            "zh" => Some(NativeLanguage::Zh),
            "ms" => Some(NativeLanguage::Ms),
            "es" => Some(NativeLanguage::Es),
            "fr" => Some(NativeLanguage::Fr),
            "de" => Some(NativeLanguage::De),
            "pt" => Some(NativeLanguage::Pt),
            "ja" => Some(NativeLanguage::Ja),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NativeLanguage::None => "none",
            NativeLanguage::Zh => "zh",
            NativeLanguage::Ms => "ms",
            NativeLanguage::Es => "es",
            NativeLanguage::Fr => "fr",
            NativeLanguage::De => "de",
            NativeLanguage::Pt => "pt",
            NativeLanguage::Ja => "ja",
        }
    }
}

impl TargetEnglish {
    /// The stored value of `targetEnglish`, or `None` for anything else.
    pub fn from_stored(value: &str) -> Option<Self> {
        match value {
            "en-US" => Some(TargetEnglish::EnUs),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TargetEnglish::EnUs => "en-US",
        }
    }
}

impl EngineSlug {
    /// The stored value of `engine`, or `None` for a slug with no adapter in
    /// the spec, such as the reserved `gector`.
    pub fn from_stored(value: &str) -> Option<Self> {
        match value {
            "languagetool" => Some(EngineSlug::Languagetool),
            "openai" => Some(EngineSlug::Openai),
            "harper" => Some(EngineSlug::Harper),
            "openrouter" => Some(EngineSlug::Openrouter),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EngineSlug::Languagetool => "languagetool",
            EngineSlug::Openai => "openai",
            EngineSlug::Harper => "harper",
            EngineSlug::Openrouter => "openrouter",
        }
    }

    /// Whether the engine sends the text off the machine (spec section 4).
    pub fn is_cloud(self) -> bool {
        matches!(self, EngineSlug::Openrouter)
    }

    /// The size limit of one Check on this Engine, in UTF-16 code units.
    ///
    /// The limit belongs to the Engine (spec section 4): the local LLM reads
    /// 2,000 units, because a longer Chunk cannot be answered inside the
    /// timeout, and every other Engine reads 5,000. The match is exhaustive on
    /// purpose, so a new slug has to name its own limit.
    pub const fn check_limit_utf16(self) -> usize {
        match self {
            EngineSlug::Openai => 2_000,
            EngineSlug::Languagetool | EngineSlug::Harper | EngineSlug::Openrouter => 5_000,
        }
    }
}

/// What one Check runs with.
///
/// Flags win over the Settings entry in `shell.json`, which wins over these
/// built-in defaults (spec section 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOptions {
    pub native: NativeLanguage,
    pub target: TargetEnglish,
    pub engine: EngineSlug,
    /// The chat endpoint of the `openai` engine. Its host must be loopback
    /// (spec section 4); the adapter, not this layer, enforces that.
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_api_key: String,
    /// The model id the `openrouter` engine asks for. Empty is `bad_arguments`.
    pub openrouter_model: String,
    /// Whether the local engine thinks before it answers (spec section 4).
    /// The adapter sends it per request, so a change needs no unit restart.
    /// It also picks the forcing route of the `openai` request; see
    /// `engines::openai::force_of`.
    pub local_thinking: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            native: NativeLanguage::None,
            target: TargetEnglish::EnUs,
            engine: EngineSlug::Languagetool,
            openai_base_url: settings::DEFAULT_OPENAI_BASE_URL.to_string(),
            openai_model: settings::DEFAULT_OPENAI_MODEL.to_string(),
            openai_api_key: String::new(),
            openrouter_model: settings::DEFAULT_OPENROUTER_MODEL.to_string(),
            local_thinking: settings::DEFAULT_LOCAL_THINKING,
        }
    }
}

impl CheckOptions {
    /// Layer the flags over the stored Settings over the built-in defaults.
    ///
    /// `shell.json` holds no key for a flag the spec does not define, so the
    /// two OpenAI text fields and the API key resolve from the file and the
    /// defaults only. The OpenRouter key is never a Setting at all: it lives
    /// in its own 0600 file (spec section 4).
    pub fn resolve(args: &CheckArgs, stored: &StoredSettings) -> Self {
        let defaults = CheckOptions::default();
        CheckOptions {
            native: args.native.or(stored.native).unwrap_or(defaults.native),
            target: args.target.or(stored.target).unwrap_or(defaults.target),
            engine: args.engine.or(stored.engine).unwrap_or(defaults.engine),
            openai_base_url: stored
                .openai_base_url
                .clone()
                .unwrap_or(defaults.openai_base_url),
            openai_model: stored.openai_model.clone().unwrap_or(defaults.openai_model),
            openai_api_key: stored
                .openai_api_key
                .clone()
                .unwrap_or(defaults.openai_api_key),
            openrouter_model: args
                .openrouter_model
                .clone()
                .or_else(|| stored.openrouter_model.clone())
                .unwrap_or(defaults.openrouter_model),
            local_thinking: args
                .thinking
                .map(Thinking::is_on)
                .or(stored.local_thinking)
                .unwrap_or(defaults.local_thinking),
        }
    }
}

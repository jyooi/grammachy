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
    Chunk,

    /// Run the interference fixture through every engine this machine reaches
    /// and print the benchmark file on stdout (spec section 13.1).
    Bench(BenchArgs),

    /// Report what this machine still needs, one line per piece.
    Doctor(DoctorArgs),
}

#[derive(Debug, Parser)]
pub struct BenchArgs {
    /// The engine the named models run on. v1 accepts openai only.
    ///
    /// This does not narrow the Engines table: one run prints the whole file.
    #[arg(long, value_enum)]
    pub engine: Option<EngineSlug>,

    /// A model to evaluate, repeatable, one Models row each.
    #[arg(long = "model", value_name = "NAME")]
    pub models: Vec<String>,
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
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EngineSlug::Languagetool => "languagetool",
            EngineSlug::Openai => "openai",
            EngineSlug::Harper => "harper",
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
        }
    }
}

impl CheckOptions {
    /// Layer the flags over the stored Settings over the built-in defaults.
    ///
    /// `shell.json` holds no key for a flag the spec does not define, so the
    /// two OpenAI text fields and the API key resolve from the file and the
    /// defaults only.
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
        }
    }
}

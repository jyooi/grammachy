//! Command line surface, spec sections 5.1, 5.2, 10, and 13.1.

use clap::{Parser, Subcommand, ValueEnum};

use crate::settings::StoredSettings;

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

    /// Report what this machine still needs, one line per piece.
    Doctor(DoctorArgs),

    /// Install the hotkeys and the menu entry, without a password.
    Setup(SetupArgs),

    /// See, install, and remove the optional engine components, without sudo.
    Engine(EngineArgs),
}

#[derive(Debug, Parser)]
pub struct EngineArgs {
    #[command(subcommand)]
    pub verb: EngineVerb,
}

#[derive(Debug, Subcommand)]
pub enum EngineVerb {
    /// One row per optional component, with what it has on disk.
    List,

    /// Fetch and unpack one component, resuming a part file it already has.
    Install(EngineNameArgs),

    /// Delete one component's installed tree, its archive, and its part file.
    Remove(EngineNameArgs),
}

#[derive(Debug, Parser)]
pub struct EngineNameArgs {
    /// The engine slug, as `grammachy engine list` prints it.
    pub slug: String,
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
    /// Take the hotkeys and the menu entry out again, keeping the weights.
    #[arg(long)]
    pub remove: bool,
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
            "harper" => Some(EngineSlug::Harper),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EngineSlug::Languagetool => "languagetool",
            EngineSlug::Harper => "harper",
        }
    }

    /// The size limit of one Check on this Engine, in UTF-16 code units.
    ///
    /// The match is exhaustive on purpose, so a new slug has to name its own
    /// limit.
    pub const fn check_limit_utf16(self) -> usize {
        match self {
            EngineSlug::Languagetool | EngineSlug::Harper => 5_000,
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
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            native: NativeLanguage::None,
            target: TargetEnglish::EnUs,
            engine: EngineSlug::Harper,
        }
    }
}

impl CheckOptions {
    /// Layer the flags over the stored Settings over the built-in defaults
    /// (spec section 7).
    pub fn resolve(args: &CheckArgs, stored: &StoredSettings) -> Self {
        let defaults = CheckOptions::default();
        CheckOptions {
            native: args.native.or(stored.native).unwrap_or(defaults.native),
            target: args.target.or(stored.target).unwrap_or(defaults.target),
            engine: args.engine.or(stored.engine).unwrap_or(defaults.engine),
        }
    }
}

//! Command line surface, spec section 5.1 and section 10.

use clap::{Parser, Subcommand, ValueEnum};

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
}

#[derive(Debug, Parser)]
pub struct CheckArgs {
    /// The language the user thinks in. Omitted means none.
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
    pub fn as_str(self) -> &'static str {
        match self {
            TargetEnglish::EnUs => "en-US",
        }
    }
}

impl EngineSlug {
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
/// built-in defaults (spec section 7). Only the flag layer exists so far; the
/// `shell.json` layer arrives with the settings ticket and fills the gap
/// between [`CheckArgs`] and this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            engine: EngineSlug::Languagetool,
        }
    }
}

impl CheckOptions {
    /// Layer the flags over the built-in defaults.
    pub fn resolve(args: &CheckArgs) -> Self {
        let defaults = CheckOptions::default();
        CheckOptions {
            native: args.native.unwrap_or(defaults.native),
            target: args.target.unwrap_or(defaults.target),
            engine: args.engine.unwrap_or(defaults.engine),
        }
    }
}

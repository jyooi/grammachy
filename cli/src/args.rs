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

    /// Run the interference fixture through every engine this machine reaches
    /// and print the benchmark file on stdout (spec section 13.1).
    Bench(BenchArgs),

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

    /// Add the Useful fix column from a judgements file, the output of
    /// `cli/bench/judge.py`. The column counts in the ranking only when the
    /// judge agrees with the committed hand labels on at least 80% of them.
    #[arg(long = "judgements", value_name = "FILE")]
    pub judgements: Option<std::path::PathBuf>,

    /// Run the 365-item eval set beside the fixture and rank the models on it.
    ///
    /// The corpus is fetched at run time into a gitignored cache and no part
    /// of it is committed (ADR 0003). A machine that cannot fill the cache
    /// prints the eval tables as skipped with a reason, never an error.
    #[arg(long = "eval-set")]
    pub eval_set: bool,

    /// The thinking mode the local rows run in: `off`, `on`, or `both`.
    ///
    /// It decides the mode of every local row, so a benchmark file is the
    /// output of its own Command line and the stored `localThinking` never
    /// moves the numbers. `both` runs every local row twice and prints both.
    #[arg(long, value_enum, default_value = "on")]
    pub thinking: BenchThinking,
}

/// The `bench --thinking` flag of `docs/spec/evals.md` section 4.1.
///
/// It is the run's own choice rather than one Check's, so it holds a third
/// value the Check flag has no meaning for: `both`, one benchmark file that
/// carries every local model in each mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum BenchThinking {
    Off,
    #[default]
    On,
    Both,
}

impl BenchThinking {
    /// The modes every local Models row runs in, in the order they print.
    ///
    /// `on` comes first under `both`, because it is the product default and
    /// the row a reader looks for.
    pub fn modes(self) -> &'static [bool] {
        match self {
            BenchThinking::Off => &[false],
            BenchThinking::On => &[true],
            BenchThinking::Both => &[true, false],
        }
    }

    /// The mode the Engines table's `openai` row runs in.
    ///
    /// That table keeps its four columns (evals spec section 4.2), so it has
    /// nowhere to name a mode and runs once. Under `both` it takes the product
    /// default, which is the engine a release is measured as shipping.
    pub fn engine_mode(self) -> bool {
        match self {
            BenchThinking::Off => false,
            BenchThinking::On | BenchThinking::Both => true,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BenchThinking::Off => "off",
            BenchThinking::On => "on",
            BenchThinking::Both => "both",
        }
    }
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

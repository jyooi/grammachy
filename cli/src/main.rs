use std::io::{self, Read, Write};
use std::process::ExitCode;

use clap::Parser;

use grammachy::args::{CheckArgs, CheckOptions, Cli, Command, EngineSlug};
use grammachy::engines::install::{self as engine_install, EngineEnvelope};
use grammachy::envelope::{Envelope, ErrorCode};
use grammachy::model::{self, ModelEnvelope};
use grammachy::settings::StoredSettings;
use grammachy::setup::{Setup, SetupEnvelope};
use grammachy::{bench, check, chunk, doctor};

/// What a run prints on stdout, already rendered.
///
/// `check` and `chunk` render one JSON envelope (spec section 5.1).
/// `bench` renders its Markdown report, and still renders the error envelope
/// when its arguments do not describe a run. A `--record` write that fails
/// after the rows ran keeps the report on stdout and exits 1.
/// `doctor` renders its report (spec section 10).
/// `setup` renders its JSON envelope (spec section 10), and so do `model`
/// (spec section 5.3) and `engine` (spec section 5.4).
struct Output {
    text: String,
    exit_code: i32,
}

impl From<Envelope> for Output {
    fn from(envelope: Envelope) -> Self {
        Output {
            text: envelope.to_json(),
            exit_code: envelope.exit_code(),
        }
    }
}

impl From<chunk::ChunkEnvelope> for Output {
    fn from(envelope: chunk::ChunkEnvelope) -> Self {
        Output {
            text: envelope.to_json(),
            exit_code: envelope.exit_code(),
        }
    }
}

impl From<doctor::DoctorOutput> for Output {
    fn from(output: doctor::DoctorOutput) -> Self {
        Output {
            text: output.text.trim_end().to_string(),
            exit_code: output.exit_code,
        }
    }
}

impl From<ModelEnvelope> for Output {
    fn from(envelope: ModelEnvelope) -> Self {
        Output {
            text: envelope.to_json(),
            exit_code: envelope.exit_code(),
        }
    }
}

impl From<EngineEnvelope> for Output {
    fn from(envelope: EngineEnvelope) -> Self {
        Output {
            text: envelope.to_json(),
            exit_code: envelope.exit_code(),
        }
    }
}

impl From<SetupEnvelope> for Output {
    fn from(envelope: SetupEnvelope) -> Self {
        Output {
            text: envelope.to_json(),
            exit_code: envelope.exit_code(),
        }
    }
}

fn main() -> ExitCode {
    let output = match run() {
        Some(output) => output,
        // clap printed help or the version already.
        None => return ExitCode::SUCCESS,
    };

    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", output.text);
    let _ = stdout.flush();

    match output.exit_code {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// `None` means clap handled the run itself, such as `--help`.
fn run() -> Option<Output> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            eprintln!("{error}");
            return Some(Envelope::error(ErrorCode::BadArguments, first_line(&error)).into());
        }
        Err(error) => {
            let _ = error.print();
            return None;
        }
    };

    match cli.command {
        Command::Check(args) => {
            let options = CheckOptions::resolve(&args, &StoredSettings::load());
            let text = match read_stdin() {
                Ok(text) => text,
                Err(message) => return Some(bad_stdin(message)),
            };
            Some(check::run(&text, &options).into())
        }
        // Chunks are packed to the selected engine's Check size limit, so
        // `chunk` resolves the engine the same way `check` does.
        Command::Chunk(args) => {
            let text = match read_stdin() {
                Ok(text) => text,
                Err(message) => return Some(bad_stdin(message)),
            };
            Some(chunk::run(&text, engine_of(args.engine).check_limit_utf16()).into())
        }
        Command::Bench(args) => Some(match bench::run(&args, &StoredSettings::load()) {
            Ok(run) => {
                let exit_code = match &run.record_failure {
                    Some(message) => {
                        eprintln!("grammachy: {message}");
                        1
                    }
                    None => 0,
                };
                Output {
                    text: run.report,
                    exit_code,
                }
            }
            Err(message) => {
                eprintln!("grammachy: {message}");
                Envelope::error(ErrorCode::BadArguments, message).into()
            }
        }),
        Command::Doctor(args) => {
            // `doctor` reads no stdin: it reports the machine, not a Selection.
            let options = CheckOptions::resolve(
                &CheckArgs {
                    native: None,
                    target: None,
                    engine: args.engine,
                    thinking: None,
                    openrouter_model: None,
                },
                &StoredSettings::load(),
            );
            let facts = doctor::Facts::collect(&options);
            Some(doctor::run(&facts, options.engine, args.json).into())
        }
        // Setup reads stdin only for `--openrouter-key`: otherwise the engine
        // and the model name come from the Settings entry, the same source a
        // Check uses (spec section 7).
        Command::Setup(args) => {
            let defaults = CheckOptions::default();
            let stored = StoredSettings::load();
            let setup = match Setup::from_env() {
                Ok(setup) => setup,
                Err(message) => return Some(SetupEnvelope::error(message).into()),
            };
            let envelope = if args.openrouter_key {
                // The key comes on stdin and never on the command line, so no
                // process list ever holds it (spec section 10).
                match read_stdin() {
                    Ok(text) => setup.write_key(&text),
                    Err(message) => SetupEnvelope::error(message),
                }
            } else if args.remove {
                setup.remove()
            } else {
                setup.install(
                    stored.engine.unwrap_or(defaults.engine),
                    &stored.openai_model.unwrap_or(defaults.openai_model),
                )
            };
            Some(envelope.into())
        }
        // `model` reads no stdin either: the verb names the model, and the
        // Settings entry says only which one the engine is currently using.
        Command::Model(args) => {
            let defaults = CheckOptions::default();
            let stored = StoredSettings::load();
            let openai_model = stored.openai_model.unwrap_or(defaults.openai_model);
            Some(model::run(&args.verb, &openai_model).into())
        }
        // `engine` reads no stdin and no Settings entry: the verb names the
        // component, and which engine a Check runs on says nothing about what
        // is on disk (spec section 5.4).
        Command::Engine(args) => Some(engine_install::run(&args.verb).into()),
    }
}

/// The engine a subcommand that takes no other Check flag runs with: the flag,
/// then the stored entry, then the default (spec section 7).
fn engine_of(flag: Option<EngineSlug>) -> EngineSlug {
    CheckOptions::resolve(
        &CheckArgs {
            native: None,
            target: None,
            engine: flag,
            thinking: None,
            openrouter_model: None,
        },
        &StoredSettings::load(),
    )
    .engine
}

fn bad_stdin(message: String) -> Output {
    eprintln!("grammachy: {message}");
    Envelope::error(ErrorCode::BadArguments, message).into()
}

/// The one-line summary of a clap error, without its "error: " prefix.
fn first_line(error: &clap::Error) -> String {
    let rendered = error.render().to_string();
    let line = rendered.lines().next().unwrap_or_default().trim();
    line.strip_prefix("error: ").unwrap_or(line).to_string()
}

fn read_stdin() -> Result<String, String> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("stdin could not be read: {error}"))?;
    String::from_utf8(bytes).map_err(|_| "stdin is not valid UTF-8.".to_string())
}

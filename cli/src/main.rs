use std::io::{self, Read, Write};
use std::process::ExitCode;

use clap::Parser;

use grammachy::args::{CheckOptions, Cli, Command};
use grammachy::envelope::{Envelope, ErrorCode};
use grammachy::settings::StoredSettings;
use grammachy::{bench, check, chunk};

/// What a run prints on stdout, already rendered.
///
/// Every subcommand but `bench` renders one JSON envelope here (spec section
/// 5.1). `bench` renders its Markdown report instead, and still renders the
/// error envelope when it fails.
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
        Command::Chunk => {
            let text = match read_stdin() {
                Ok(text) => text,
                Err(message) => return Some(bad_stdin(message)),
            };
            Some(chunk::run(&text).into())
        }
        Command::Bench(args) => Some(match bench::run(&args, &StoredSettings::load()) {
            Ok(report) => Output {
                text: report,
                exit_code: 0,
            },
            Err(message) => {
                eprintln!("grammachy: {message}");
                Envelope::error(ErrorCode::BadArguments, message).into()
            }
        }),
    }
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

use std::io::{self, Read, Write};
use std::process::ExitCode;

use clap::Parser;

use grammachy::args::{CheckOptions, Cli, Command};
use grammachy::check;
use grammachy::envelope::{Envelope, ErrorCode};

fn main() -> ExitCode {
    let envelope = match run() {
        Some(envelope) => envelope,
        // clap printed help or the version already.
        None => return ExitCode::SUCCESS,
    };

    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", envelope.to_json());
    let _ = stdout.flush();

    match envelope.exit_code() {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

/// `None` means clap handled the run itself, such as `--help`.
fn run() -> Option<Envelope> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            eprintln!("{error}");
            return Some(Envelope::error(ErrorCode::BadArguments, first_line(&error)));
        }
        Err(error) => {
            let _ = error.print();
            return None;
        }
    };

    match cli.command {
        Command::Check(args) => {
            let options = CheckOptions::resolve(&args);
            let text = match read_stdin() {
                Ok(text) => text,
                Err(message) => {
                    eprintln!("grammachy: {message}");
                    return Some(Envelope::error(ErrorCode::BadArguments, message));
                }
            };
            Some(check::run(&text, &options))
        }
    }
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

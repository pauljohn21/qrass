//! `qrass` CLI: compile one SCSS/CSS entry to stdout or `-o` file.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tracing::error;

use qrass::error::SassError;
use qrass::{Options, compile};

struct Cli {
    input: PathBuf,
    output: Option<PathBuf>,
    options: Options,
}

fn parse_style(value: &str) -> Result<qrass::OutputStyle, SassError> {
    match value {
        "expanded" => Ok(qrass::OutputStyle::Expanded),
        other => Err(SassError::new(
            qrass::error::ErrorKind::Unsupported,
            format!("style '{other}' is not supported (only 'expanded')"),
        )),
    }
}

fn parse_args() -> Result<Cli, SassError> {
    let unsupported = |msg: String| SassError::new(qrass::error::ErrorKind::Unsupported, msg);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut style = qrass::OutputStyle::Expanded;
    let mut load_paths: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                let Some(path) = args.next() else {
                    return Err(unsupported("missing value for -o".into()));
                };
                output = Some(PathBuf::from(path));
            }
            "--style" => {
                let Some(value) = args.next() else {
                    return Err(unsupported("missing value for --style".into()));
                };
                style = parse_style(&value)?;
            }
            "--load-path" => {
                let Some(path) = args.next() else {
                    return Err(unsupported("missing value for --load-path".into()));
                };
                load_paths.push(PathBuf::from(path));
            }
            flag if flag.starts_with('-') => {
                return Err(unsupported(format!("unknown option '{flag}'")));
            }
            path if input.is_none() => input = Some(PathBuf::from(path)),
            extra => return Err(unsupported(format!("unexpected argument '{extra}'"))),
        }
    }
    let input = input.ok_or_else(|| unsupported("no input file given".into()))?;
    Ok(Cli {
        input,
        output,
        options: Options { style, load_paths },
    })
}

fn run() -> Result<(), SassError> {
    let cli = parse_args()?;
    let css = compile(Path::new(&cli.input), &cli.options)?;
    match cli.output {
        Some(path) => {
            std::fs::write(&path, css)
                .map_err(|err| SassError::io(err, &path.display().to_string()))?;
        }
        None => std::io::stdout()
            .write_all(css.as_bytes())
            .map_err(|err| SassError::io(err, "<stdout>"))?,
    }
    Ok(())
}

fn main() -> ExitCode {
    qrass::tracing_setup::init();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err}");
            ExitCode::FAILURE
        }
    }
}

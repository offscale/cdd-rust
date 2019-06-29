#[macro_use]
extern crate serde_derive;

use colored::Colorize;
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    ffi::OsStr,
    fmt::{
        self,
        Display,
    },
    fs,
    io::{
        self,
        Write,
    },
    ops::Not,
    path::{
        Path,
        PathBuf,
    },
    process,
};

mod constructor;
mod openapi;
use constructor::*;

pub(crate) enum Error {
    IncorrectUsage,
    ReadFile(io::Error),
    ParseFile {
        error: syn::Error,
        filepath: PathBuf,
        source_code: String,
    },
    GenerationError(GenerationError),
}

impl From<GenerationError> for Error {
    fn from(e: GenerationError) -> Self {
        Error::GenerationError(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match self {
            IncorrectUsage => {
                write!(f, "Usage: dump-syntax path/to/filename.rs")
            }
            ReadFile(error) => write!(f, "Unable to read file: {}", error),
            ParseFile {
                error,
                filepath,
                source_code,
            } => render_location(f, error, filepath, source_code),
            GenerationError(e) => write!(f, "{:?}", e),
        }
    }
}

fn render_location(
    formatter: &mut fmt::Formatter,
    err: &syn::Error,
    filepath: &Path,
    code: &str,
) -> fmt::Result {
    let start = err.span().start();
    let mut end = err.span().end();

    if start.line == end.line && start.column == end.column {
        return render_fallback(formatter, err);
    }

    let code_line = match code.lines().nth(start.line - 1) {
        Some(line) => line,
        None => return render_fallback(formatter, err),
    };

    if end.line > start.line {
        end.line = start.line;
        end.column = code_line.len();
    }

    let filename = filepath
        .file_name()
        .map(OsStr::to_string_lossy)
        .unwrap_or(Cow::Borrowed("main.rs"));

    write!(
        formatter,
        "\n\
         {error}{header}\n\
         {indent}{arrow} {filename}:{linenum}:{colnum}\n\
         {indent} {pipe}\n\
         {label} {pipe} {code}\n\
         {indent} {pipe} {offset}{underline} {message}\n\
         ",
        error = "error".red().bold(),
        header = ": Syn unable to parse file".bold(),
        indent = " ".repeat(start.line.to_string().len()),
        arrow = "-->".blue().bold(),
        filename = filename,
        linenum = start.line,
        colnum = start.column,
        pipe = "|".blue().bold(),
        label = start.line.to_string().blue().bold(),
        code = code_line.trim_end(),
        offset = " ".repeat(start.column),
        underline = "^".repeat(end.column - start.column).red().bold(),
        message = err.to_string().red(),
    )
}

fn render_fallback(
    formatter: &mut fmt::Formatter,
    err: &syn::Error,
) -> fmt::Result {
    write!(formatter, "Unable to parse file: {}", err)
}

fn generate(file: &str) -> Result<(), Error> {
    let filepath = PathBuf::from(file);

    let code = fs::read_to_string(&filepath).map_err(Error::ReadFile)?;
    let syntax = syn::parse_file(&code).map_err({
        |error| {
            Error::ParseFile {
                error,
                filepath,
                source_code: code,
            }
        }
    })?;
    let mut constructor = Constructor::new();
    syn::visit::visit_file(&mut constructor, &syntax);

    for document in constructor.open_api_documents()? {
        println!(
            "{}",
            serde_yaml::to_string(&document)
                .expect("error serializaing to serder")
        );
    }
    Ok(())
}

pub fn do_generate(file: &str) -> Result<(), String> {
    generate(file).map_err(|e| e.to_string())
}

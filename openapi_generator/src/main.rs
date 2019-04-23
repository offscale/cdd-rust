#[macro_use]
extern crate serde_derive;

use colored::Colorize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::process;

mod constructor;
use constructor::*;

#[derive(Debug, Serialize)]
struct OpenApiDocument {
    swagger: String,
    info: OpenApiInfo,
    paths: Vec<()>,
    components: Vec<OpenApiComponents>,
}

#[derive(Debug, Serialize)]
struct OpenApiInfo {
    title: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct OpenApiComponents {
    schemas: HashMap<String, OpenApiSchema>,
}

#[derive(Debug, Serialize)]
struct OpenApiSchema {
    required: Vec<String>,
    properties: HashMap<String, OpenApiProperties>,
}

#[derive(Clone, Debug, Serialize)]
struct OpenApiProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<OpenApiProperties>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    o_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "$ref")]
    dollar_ref: Option<String>,
    #[serde(skip_serializing_if = "Not::not")]
    #[serde(rename = "uniqueItems")]
    unique_items: bool,
}

impl OpenApiProperties {
    fn new(o_type: String) -> OpenApiProperties {
        OpenApiProperties {
            dollar_ref: None,
            format: None,
            items: None,
            minimum: None,
            o_type: Some(o_type),
            unique_items: false,
        }
    }
    fn new_ref(dollar_ref: String) -> OpenApiProperties {
        OpenApiProperties {
            dollar_ref: Some(dollar_ref),
            format: None,
            items: None,
            minimum: None,
            o_type: None,
            unique_items: false,
        }
    }
    fn with_minimum(&self, minimum: i64) -> Self {
        let mut new = self.clone();
        new.minimum = Some(minimum);
        new
    }
    fn with_format(&self, format: String) -> Self {
        let mut new = self.clone();
        new.format = Some(format);
        new
    }
    fn with_items(&self, items: OpenApiProperties) -> Self {
        let mut new = self.clone();
        new.items = Some(Box::new(items));
        new
    }
    fn with_unique_items(&self) -> Self {
        let mut new = self.clone();
        new.unique_items = true;
        new
    }
}

enum Error {
    IncorrectUsage,
    ReadFile(io::Error),
    ParseFile {
        error: syn::Error,
        filepath: PathBuf,
        source_code: String,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match self {
            IncorrectUsage => write!(f, "Usage: dump-syntax path/to/filename.rs"),
            ReadFile(error) => write!(f, "Unable to read file: {}", error),
            ParseFile {
                error,
                filepath,
                source_code,
            } => render_location(f, error, filepath, source_code),
        }
    }
}

fn main() {
    if let Err(error) = try_main() {
        let _ = writeln!(io::stderr(), "{}", error);
        process::exit(1);
    }
}

fn try_main() -> Result<(), Error> {
    let mut args = env::args_os();
    let _ = args.next(); // executable name

    let filepath = match (args.next(), args.next()) {
        (Some(arg), None) => PathBuf::from(arg),
        _ => return Err(Error::IncorrectUsage),
    };

    let code = fs::read_to_string(&filepath).map_err(Error::ReadFile)?;
    let syntax = syn::parse_file(&code).map_err({
        |error| Error::ParseFile {
            error,
            filepath,
            source_code: code,
        }
    })?;
    let mut constructor = Constructor::new();
    syn::visit::visit_file(&mut constructor, &syntax);

    for document in constructor.open_api_documents().unwrap() {
        println!("{}", serde_yaml::to_string(&document).unwrap());
    }
    Ok(())
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

fn render_fallback(formatter: &mut fmt::Formatter, err: &syn::Error) -> fmt::Result {
    write!(formatter, "Unable to parse file: {}", err)
}

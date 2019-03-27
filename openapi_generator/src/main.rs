#[macro_use]
extern crate serde_derive;

use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Write};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process;

use colored::Colorize;

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

#[derive(Debug, Serialize)]
struct OpenApiProperties {
    format: Option<String>,
    #[serde(rename = "type")]
    o_type: String,
}

enum Error {
    IncorrectUsage,
    ReadFile(io::Error),
    ParseFile {
        error: syn::Error,
        filepath: PathBuf,
        source_code: String,
    },
    NoStruct,
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Error::*;

        match self {
            IncorrectUsage => write!(f, "Usage: dump-syntax path/to/filename.rs"),
            ReadFile(error) => write!(f, "Unable to read file: {}", error),
            NoStruct => write!(f, "The file has no struct"),
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
    let first_struct = syntax.items.into_iter().map(|i| {
        if let syn::Item::Struct(s) = i {
            Some(s)
        } else {
            None
        }
    })
        .filter(|v| v.is_some())
        .map(|v| v.unwrap())
        .next()
        .ok_or(Error::NoStruct)?;

    let struct_name = first_struct.ident.to_string();
    let mut required = Vec::new();
    let mut properties = HashMap::new();

    for f in first_struct.fields.iter().filter(|f| f.ident.is_some()) {
        let ident = f.ident.clone().unwrap().to_string();
        match &f.ty {
            syn::Type::Path(p) => {
                let f_segment = &p.path.segments[0];
                match f_segment.ident.to_string().as_str() {
                    "usize" => {
                        properties.insert(ident.clone(), OpenApiProperties {
                            o_type: "integer".to_owned(),
                            format: Some("int32".to_owned()),
                        });
                        required.push(ident.clone());
                    }
                    "String" => {
                        properties.insert(ident.clone(), OpenApiProperties {
                            o_type: "string".to_owned(),
                            format: None,
                        });
                        required.push(ident.clone());
                    }
                    "Option" => {
                        match &f_segment.arguments {
                            syn::PathArguments::AngleBracketed(args) => {
                                match &args.args[0] {
                                    syn::GenericArgument::Type(t) => {
                                        if let syn::Type::Path(t) = t {
                                            let s = &t.path.segments[0].ident;
                                            match s.to_string().as_str() {
                                                "u64" => {
                                                    properties.insert(ident.clone(), OpenApiProperties {
                                                        o_type: "integer".to_owned(),
                                                        format: Some("int64".to_owned()),
                                                    });
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    "Vec" => {
                        match &f_segment.arguments {
                            syn::PathArguments::AngleBracketed(args) => {
                                match &args.args[0] {
                                    syn::GenericArgument::Type(t) => {
                                        if let syn::Type::Path(t) = t {
                                            let s = &t.path.segments[0].ident;
                                            match s.to_string().as_str() {
                                                "char" => {
                                                    properties.insert(ident.clone(), OpenApiProperties {
                                                        o_type: "array".to_owned(),
                                                        format: None,
                                                    });
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let mut schemas = HashMap::new();
    schemas.insert(struct_name, OpenApiSchema {
        required,
        properties,
    });

    let document = OpenApiDocument {
        swagger: "3.0".to_owned(),
        info: OpenApiInfo {
            title: "Autogenerated OpenAPI model".to_owned(),
            version: "1.0".to_owned(),
        },
        paths: Vec::new(),
        components: vec![OpenApiComponents {
            schemas,
        }],
    };
    println!("{}", serde_yaml::to_string(&document).unwrap());
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
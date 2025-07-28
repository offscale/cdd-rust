use clap::Parser;

/// A CLI to generate Rust code from OpenAPI specifications and vice-versa.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Cli {
    /// Generate Rust code from an OpenAPI specification.
    FromOpenapi(FromOpenapi),
    /// Generate an OpenAPI specification from Rust code.
    ToOpenapi(ToOpenapi),
}

#[derive(Parser, Debug)]
struct FromOpenapi {
    /// The path to the OpenAPI specification file.
    #[arg(short, long)]
    input: std::path::PathBuf,

    /// The path to the output directory for the models.
    #[arg(short, long)]
    output: std::path::PathBuf,

    /// The path to the output directory for the schema.
    #[arg(short, long)]
    schema_output: std::path::PathBuf,
}

#[derive(Parser, Debug)]
struct ToOpenapi {
    /// The path to the Rust source code file.
    #[arg(short, long)]
    input: std::path::PathBuf,

    /// The path to the output file.
    #[arg(short, long)]
    output: std::path::PathBuf,
}

use cdd_rust::{from_openapi, to_openapi};

fn main() {
    let cli = Cli::parse();

    match cli {
        Cli::FromOpenapi(args) => {
            if let Err(e) = from_openapi::generate(args.input, args.output, args.schema_output) {
                eprintln!("Error generating Rust code from OpenAPI: {}", e);
                std::process::exit(1);
            }
        }
        Cli::ToOpenapi(args) => {
            if let Err(e) = to_openapi::generate(args.input, args.output) {
                eprintln!("Error generating OpenAPI spec from Rust: {}", e);
                std::process::exit(1);
            }
        }
    }
}

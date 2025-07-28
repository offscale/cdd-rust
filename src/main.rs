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

    /// The path to the output directory.
    #[arg(short, long)]
    output: std::path::PathBuf,
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

fn main() {
    let cli = Cli::parse();

    match cli {
        Cli::FromOpenapi(args) => {
            println!("Generating Rust code from {:?}", args.input);
        }
        Cli::ToOpenapi(args) => {
            println!("Generating OpenAPI spec from {:?}", args.input);
        }
    }
}

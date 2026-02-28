use cdd_core::AppResult;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ToOpenApiArgs {
    #[clap(short, long)]
    pub file: PathBuf,
}

pub fn execute(args: &ToOpenApiArgs) -> AppResult<()> {
    println!("to_openapi executed with file: {:?}", args.file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_openapi_execute() {
        let args = ToOpenApiArgs {
            file: PathBuf::from("dummy"),
        };
        let result = execute(&args);
        assert!(result.is_ok());
    }
}

use cdd_core::AppResult;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct FromOpenApiArgs {
    #[clap(short, long)]
    pub input: PathBuf,
}

pub fn execute(args: &FromOpenApiArgs) -> AppResult<()> {
    println!("from_openapi executed with input: {:?}", args.input);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_openapi_execute() {
        let args = FromOpenApiArgs {
            input: PathBuf::from("dummy"),
        };
        let result = execute(&args);
        assert!(result.is_ok());
    }
}

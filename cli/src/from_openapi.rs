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

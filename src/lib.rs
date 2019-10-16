//#![deny(warnings)]
#[macro_use]
extern crate serde_derive;

use constructor::Constructor;
use error::Error;
use std::{fs, path::PathBuf};

mod constructor;
mod error;
mod openapi;
mod parser;
mod project_reader;
mod project;

pub use project_reader::ProjectReader;

fn generate(file: &str) -> Result<(), Error> {
    let filepath = PathBuf::from(file);
    // let path: String = filepath.into_os_string().into_string().unwrap();
    let code = fs::read_to_string(&filepath).map_err(Error::ReadFile)?;
    let reader = ProjectReader::read(filepath.clone());
    let syntax = syn::parse_file(&code).map_err({
        |error| Error::ParseFile {
            error,
            filepath,
            source_code: code,
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

#![deny(warnings)]
extern crate openapi_generator;

use std::{
    io::{
        self,
        Write,
    },
    process,
};

fn main() {
    let file = "examples/diesel.rs";
    if let Err(error) = openapi_generator::do_generate(file) {
        let _ = writeln!(io::stderr(), "{}", error);
        process::exit(1);
    }
}

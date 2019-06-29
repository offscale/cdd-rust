#[macro_use]
extern crate serde_derive;
extern crate openapi_generator;

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

fn main() {
    let file = "examples/diesel.rs";
    if let Err(error) = openapi_generator::do_generate(file) {
        let _ = writeln!(io::stderr(), "{}", error);
        process::exit(1);
    }
}

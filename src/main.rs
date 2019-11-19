#![deny(warnings)]
extern crate cdd_rust;

use std::{
    io::{
        self,
        Write,
    },
    process,
};

fn main() {
    let file = "template/diesel.rs";
    if let Err(error) = cdd_rust::do_generate(file) {
        let _ = writeln!(io::stderr(), "{}", error);
        process::exit(1);
    }
}

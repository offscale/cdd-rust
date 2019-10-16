use cdd_rust::*;
use std::path::PathBuf;

fn main() {
    let path = PathBuf::from("examples");
    if let Ok(project) = ProjectReader::read(path) {
        println!("{:?}", project.specfile);
    } else {
        println!("error");
    }
}

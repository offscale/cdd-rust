use crate::error;
use openapiv3::OpenAPI;
use std::{
    fs,
    path::PathBuf,
};

use crate::project::*;

use std::{
    io::{
        self,
        Write,
    },
    process,
};

pub struct ProjectReader {
    pub specfile: OpenAPI,
}

impl ProjectReader {
    pub fn read(path: PathBuf) -> Result<Self, std::io::Error> {
        let mut spec = path;
        spec.push("openapi.yml");
        let spec = fs::read_to_string(spec)?;
        let openapi: OpenAPI =
            serde_yaml::from_str(&spec).expect("Could not deserialize input");

        let spec_project = Project::parse_yml(openapi.clone());
        println!("{}", spec_project.info.endpoint);
        Ok(ProjectReader { specfile: openapi })
    }
}

use openapiv3::OpenAPI;
use std::fs;
use std::path::PathBuf;

pub struct Project {
    pub specfile: OpenAPI,
}

impl Project {
    pub fn read(path: PathBuf) -> Result<Self, crate::Error> {
        let mut spec = path;
        spec.push("openapi.yml");
        let spec = fs::read_to_string(spec).unwrap();

        let openapi: OpenAPI =
            serde_yaml::from_str(&spec).expect("Could not deserialize input");

        Ok(Project { specfile: openapi })
    }
}

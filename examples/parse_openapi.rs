extern crate cdd_rust;

use openapiv3::OpenAPI;
use serde_yaml;

fn main() {
    let data = include_str!("openapi.yml");
    let openapi: OpenAPI =
        serde_yaml::from_str(data).expect("Could not deserialize input");
    println!("{:?}", openapi);
}

extern crate cdd_rust;

use serde_yaml;
use openapiv3::OpenAPI;

fn main() {
    let data = include_str!("openapi.yml");
    let openapi: OpenAPI = serde_yaml::from_str(data).expect("Could not deserialize input");
    println!("{:?}", openapi);
}

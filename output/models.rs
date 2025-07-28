#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DummySchema {
    pub id: i64,
    pub name: String,
}

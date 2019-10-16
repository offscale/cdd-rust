use super::*;

#[derive(Debug)]
pub struct Model {
    pub name: String,
    pub fields: Vec<Box<Variable>>,
}

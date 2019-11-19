#[derive(Debug)]
pub struct Variable {
    pub name: String,
    pub variable_type: VariableType,
    pub optional: bool,
    pub value: Option<String>,
}

#[derive(Debug)]
pub enum VariableType {
    StringType,
    IntType,
    BoolType,
    FloatType,
    ArrayType(Box<VariableType>),
    ComplexType(String),
}

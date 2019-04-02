use crate::*;
use std::collections::{HashSet, HashMap};
use syn::{Field, Fields, ItemStruct, TypePath, AngleBracketedGenericArguments};
use syn::visit::{visit_fields_named, visit_generic_argument, visit_path_arguments, visit_type, Visit};

struct StructProperties {
    required: HashSet<String>,
    properties: HashMap<String, OpenApiProperties>,
    ident: Option<String>,
}

impl<'ast> Visit<'ast> for StructProperties {
    fn visit_angle_bracketed_generic_arguments(&mut self, i: &'ast AngleBracketedGenericArguments) {
        visit_generic_argument(self, &i.args[0]);
    }
    fn visit_field(&mut self, f: &'ast Field) {
        let ident = f.ident.clone().unwrap().to_string();
        self.ident = Some(ident);
        visit_type(self, &f.ty);
    }
    fn visit_type_path(&mut self, p: &'ast TypePath) {
        let f_segment = &p.path.segments[0];
        let mut next_required = false;
        match f_segment.ident.to_string().as_str() {
            "bool" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("boolean".to_owned())
                );
                next_required = true;
            }
            "usize" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned())
                        .with_minimum(0),
                );
                next_required = true;
            }
            "u32" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned())
                        .with_minimum(0),
                );
                next_required = true;
            }
            "u64" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int64".to_owned())
                        .with_minimum(0),
                );
                next_required = true;
            }
            "isize" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned()).with_format("int32".to_owned()),
                );
                next_required = true;
            }
            "i32" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned()).with_format("int32".to_owned()),
                );
                next_required = true;
            }
            "i64" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned()).with_format("int64".to_owned()),
                );
                next_required = true;
            }
            "f32" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("number".to_owned()).with_format("float".to_owned()),
                );
                next_required = true;
            }
            "f64" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("number".to_owned()).with_format("double".to_owned()),
                );
                next_required = true;
            }
            "String" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("string".to_owned()),
                );
                next_required = true;
            }
            "char" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("array".to_owned()),
                );
                next_required = true;
            }
            "Option" => {
                visit_path_arguments(self, &f_segment.arguments);
                self.required.remove(&self.ident.clone().unwrap());
            }
            "Vec" => {
                visit_path_arguments(self, &f_segment.arguments);
            }
            t => panic!("Type {} can't be translated to openAPI models.", t),
        }
        if next_required {
            self.required.insert(self.ident.clone().unwrap());
        }
    }
}

#[derive(Debug)]
pub(crate) enum GenerationError {
    NoNamedFields,
}

fn struct_to_open_api_document(rust_struct: &ItemStruct) -> Result<OpenApiDocument, GenerationError> {
    let struct_name = rust_struct.ident.to_string();
    if let Fields::Named(fields_named) = &rust_struct.fields {
        let mut struct_properties = StructProperties {
            required: HashSet::new(),
            properties: HashMap::new(),
            ident: None,
        };
        visit_fields_named(&mut struct_properties, fields_named);
        let mut schemas = HashMap::new();
        schemas.insert(struct_name, OpenApiSchema {
            properties: struct_properties.properties.clone(),
            required: struct_properties.required.into_iter().collect(),
        });

        Ok(OpenApiDocument {
            swagger: "3.0".to_owned(),
            info: OpenApiInfo {
                title: "Autogenerated OpenAPI model".to_owned(),
                version: "1.0".to_owned(),
            },
            paths: Vec::new(),
            components: vec![OpenApiComponents {
                schemas,
            }],
        })
    } else {
        Err(GenerationError::NoNamedFields)
    }
}

pub(crate) struct Constructor {
    structs: Vec<ItemStruct>,
}

impl Constructor {
    pub(crate) fn new() -> Constructor {
        Constructor {
            structs: Vec::new(),
        }
    }

    pub(crate) fn open_api_documents(&self) -> Result<Vec<OpenApiDocument>, GenerationError> {
        self.structs.iter().map(|s| struct_to_open_api_document(s)).collect()
    }
}

impl<'ast> Visit<'ast> for Constructor {
    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        self.structs.push(i.clone());
    }
}
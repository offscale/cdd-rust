use crate::openapi::{
    OpenApiComponents,
    OpenApiDocument,
    OpenApiInfo,
    OpenApiProperties,
    OpenApiSchema,
};
use proc_macro2::{
    Delimiter,
    TokenTree,
};
use std::collections::{
    HashMap,
    HashSet,
};
use syn::{
    visit::{
        visit_fields_named,
        visit_generic_argument,
        visit_path_arguments,
        visit_type,
        Visit,
    },
    AngleBracketedGenericArguments,
    Field,
    Fields,
    ItemMacro,
    ItemStruct,
    Macro,
    TypePath,
};

struct StructProperties {
    required: HashSet<String>,
    properties: HashMap<String, OpenApiProperties>,
    ident: Option<String>,
}

impl StructProperties {}

impl<'ast> Visit<'ast> for StructProperties {
    fn visit_angle_bracketed_generic_arguments(
        &mut self,
        i: &'ast AngleBracketedGenericArguments,
    ) {
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
                    OpenApiProperties::new("boolean".to_owned()),
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
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned()),
                );
                next_required = true;
            }
            "i32" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned()),
                );
                next_required = true;
            }
            "i64" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("integer".to_owned())
                        .with_format("int64".to_owned()),
                );
                next_required = true;
            }
            "f32" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("number".to_owned())
                        .with_format("float".to_owned()),
                );
                next_required = true;
            }
            "f64" => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new("number".to_owned())
                        .with_format("double".to_owned()),
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
                    OpenApiProperties::new("char".to_owned()),
                );
                next_required = true;
            }
            "Option" => {
                visit_path_arguments(self, &f_segment.arguments);
                self.required.remove(&self.ident.clone().unwrap());
            }
            "Vec" => {
                visit_path_arguments(self, &f_segment.arguments);
                let ident = self.ident.clone().unwrap();
                let new_props = OpenApiProperties::new("array".to_owned())
                    .with_items(self.properties[&ident].clone());
                self.properties.insert(ident, new_props);
            }
            "HashSet" | "BTreeSet" => {
                visit_path_arguments(self, &f_segment.arguments);
                let ident = self.ident.clone().unwrap();
                let new_props = OpenApiProperties::new("array".to_owned())
                    .with_unique_items()
                    .with_items(self.properties[&ident].clone());
                self.properties.insert(ident, new_props);
            }
            dollar_ref => {
                self.properties.insert(
                    self.ident.clone().unwrap(),
                    OpenApiProperties::new_ref(format!(
                        "#/components/schemas/{}",
                        dollar_ref
                    )),
                );
                next_required = true;
            }
        }
        if next_required {
            self.required.insert(self.ident.clone().unwrap());
        }
    }
}

#[derive(Debug)]
pub enum GenerationError {
    ExpectingArrow,
    ExpectingComma,
    ExpectingSqlType,
    InvalidSqlType,
    NoNamedFields,
    NoFields,
}

fn struct_to_open_api_document(
    rust_struct: &ItemStruct,
) -> Result<OpenApiDocument, GenerationError> {
    let struct_name = rust_struct.ident.to_string();
    if let Fields::Named(fields_named) = &rust_struct.fields {
        let mut struct_properties = StructProperties {
            required: HashSet::new(),
            properties: HashMap::new(),
            ident: None,
        };
        visit_fields_named(&mut struct_properties, fields_named);
        let mut schemas = HashMap::new();
        schemas.insert(
            struct_name,
            OpenApiSchema {
                properties: struct_properties.properties.clone(),
                required: struct_properties.required.into_iter().collect(),
            },
        );

        Ok(OpenApiDocument {
            swagger: "3.0".to_owned(),
            info: OpenApiInfo {
                title: "Autogenerated OpenAPI model".to_owned(),
                version: "1.0".to_owned(),
            },
            paths: Vec::new(),
            components: vec![OpenApiComponents { schemas }],
        })
    } else {
        Err(GenerationError::NoNamedFields)
    }
}

fn parse_arrow<I: Iterator<Item = TokenTree>>(
    s: &mut I,
) -> Result<(), GenerationError> {
    if let Some(TokenTree::Punct(p)) = s.next() {
        if p.as_char() == '-' {
            if let Some(TokenTree::Punct(p)) = s.next() {
                if p.as_char() == '>' {
                    Ok(())
                } else {
                    Err(GenerationError::ExpectingArrow)
                }
            } else {
                Err(GenerationError::ExpectingArrow)
            }
        } else {
            Err(GenerationError::ExpectingArrow)
        }
    } else {
        Err(GenerationError::ExpectingArrow)
    }
}

fn parse_comma<I: Iterator<Item = TokenTree>>(
    s: &mut I,
) -> Result<(), GenerationError> {
    if let Some(TokenTree::Punct(p)) = s.next() {
        if p.as_char() == ',' {
            Ok(())
        } else {
            Err(GenerationError::ExpectingComma)
        }
    } else {
        Err(GenerationError::ExpectingComma)
    }
}

fn macro_to_open_api_document(
    diesel_macro: &Macro,
) -> Result<OpenApiDocument, GenerationError> {
    let mut tts = diesel_macro.tts.clone().into_iter();
    let mut schemas = HashMap::new();
    let struct_name = match tts.next() {
        Some(TokenTree::Ident(i)) => Ok(i.to_string()),
        _ => Err(GenerationError::NoNamedFields),
    }?;
    if let Some(TokenTree::Group(g)) = tts.next() {
        let mut required = Vec::new();
        let actual_group = if g.delimiter() == Delimiter::Brace {
            g
        } else {
            for t in g.stream() {
                if let TokenTree::Ident(r) = t {
                    required.push(r.to_string());
                }
            }
            match &tts.next() {
                Some(TokenTree::Group(ng))
                    if ng.delimiter() == Delimiter::Brace =>
                {
                    Ok(ng.clone())
                }
                _ => Err(GenerationError::NoFields),
            }?
        };
        let mut item_pairs: Vec<(String, String)> = Vec::new();
        let mut items_stream = actual_group.stream().clone().into_iter();
        while let Some(TokenTree::Ident(id)) = items_stream.next() {
            parse_arrow(&mut items_stream)?;
            let item_type =
                if let Some(TokenTree::Ident(it)) = items_stream.next() {
                    Ok(it.to_string())
                } else {
                    Err(GenerationError::ExpectingSqlType)
                }?;
            item_pairs.push((id.to_string(), item_type));
            parse_comma(&mut items_stream)?;
        }
        let mut properties = HashMap::new();
        for (name, field_type) in item_pairs.into_iter() {
            let field_properties = match field_type.as_str() {
                "BigInt" => {
                    Ok(OpenApiProperties::new("integer".to_owned())
                        .with_format("int64".to_owned()))
                }
                "Binary" => {
                    Ok(OpenApiProperties::new("array".to_owned()).with_items(
                        OpenApiProperties::new("integer".to_owned())
                            .with_format("int32".to_owned()),
                    ))
                }
                "Bool" => Ok(OpenApiProperties::new("boolean".to_owned())),
                "Double" => {
                    Ok(OpenApiProperties::new("number".to_owned())
                        .with_format("double".to_owned()))
                }
                "Float" => {
                    Ok(OpenApiProperties::new("number".to_owned())
                        .with_format("float".to_owned()))
                }
                "Integer" => {
                    Ok(OpenApiProperties::new("integer".to_owned())
                        .with_format("int64".to_owned()))
                }
                "Numeric" => {
                    Ok(OpenApiProperties::new("number".to_owned())
                        .with_format("double".to_owned()))
                }
                "SmallInt" => {
                    Ok(OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned()))
                }
                "Text" => Ok(OpenApiProperties::new("string".to_owned())),
                "TinyInt" => {
                    Ok(OpenApiProperties::new("integer".to_owned())
                        .with_format("int32".to_owned()))
                }
                _ => Err(GenerationError::InvalidSqlType),
            }?;
            properties.insert(name, field_properties);
        }
        schemas.insert(
            struct_name,
            OpenApiSchema {
                properties,
                required,
            },
        );
    } else {
        schemas.insert(
            struct_name,
            OpenApiSchema {
                properties: HashMap::new(),
                required: Vec::new(),
            },
        );
    };
    let document = OpenApiDocument {
        swagger: "3.0".to_owned(),
        info: OpenApiInfo {
            title: "Autogenerated OpenAPI model".to_owned(),
            version: "1.0".to_owned(),
        },
        paths: Vec::new(),
        components: vec![OpenApiComponents { schemas }],
    };
    Ok(document)
}

pub(crate) struct Constructor {
    structs: Vec<ItemStruct>,
    diesel_macros: Vec<ItemMacro>,
}

impl Constructor {
    pub(crate) fn new() -> Constructor {
        Constructor {
            diesel_macros: Vec::new(),
            structs: Vec::new(),
        }
    }

    pub(crate) fn open_api_documents(
        &self,
    ) -> Result<Vec<OpenApiDocument>, GenerationError> {
        let mut diesel_macros: Vec<OpenApiDocument> = self
            .diesel_macros
            .iter()
            .map(|m| macro_to_open_api_document(&m.mac))
            .collect::<Result<Vec<OpenApiDocument>, GenerationError>>()?;
        let native_structs = self
            .structs
            .iter()
            .map(|s| struct_to_open_api_document(s))
            .collect::<Result<Vec<OpenApiDocument>, GenerationError>>()?;
        diesel_macros.extend(native_structs);
        Ok(diesel_macros)
    }
}

impl<'ast> Visit<'ast> for Constructor {
    fn visit_item_macro(&mut self, i: &'ast ItemMacro) {
        if i.mac.path.segments[0].ident.to_string().as_str() == "table" {
            self.diesel_macros.push(i.clone());
        }
    }

    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        self.structs.push(i.clone());
    }
}

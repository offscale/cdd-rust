use indexmap::IndexMap;
use openapiv3::{
    Components, Info, IntegerFormat, IntegerType, OpenAPI, Paths, ReferenceOr, Schema, SchemaKind,
    StringType, Type, VariantOrUnknownOrEmpty,
};
use std::fs;
use std::path::Path;
use syn::{
    visit::{Visit},
    ItemStruct, TypePath,
};

struct Visitor {
    schemas: IndexMap<String, ReferenceOr<Schema>>,
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        let mut properties = IndexMap::new();
        let mut fields: Vec<_> = i.fields.iter().collect();
        fields.sort_by_key(|f| f.ident.as_ref().unwrap().to_string());
        for field in fields {
            if let Some(ident) = &field.ident {
                let ty = get_schema_from_type(&field.ty);
                let ty = match ty {
                    ReferenceOr::Item(schema) => ReferenceOr::Item(Box::new(schema)),
                    ReferenceOr::Reference { reference } => ReferenceOr::Reference { reference },
                };
                properties.insert(ident.to_string(), ty);
            }
        }
        self.schemas.insert(
            i.ident.to_string(),
            ReferenceOr::Item(Schema {
                schema_data: Default::default(),
                schema_kind: SchemaKind::Type(Type::Object(openapiv3::ObjectType {
                    properties,
                    ..Default::default()
                })),
            }),
        );
    }
}

fn get_schema_from_type(ty: &syn::Type) -> ReferenceOr<Schema> {
    if let syn::Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            let type_name = segment.ident.to_string();
            return match type_name.as_str() {
                "String" => ReferenceOr::Item(Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::String(StringType::default())),
                }),
                "i64" => ReferenceOr::Item(Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::Integer(IntegerType {
                        format: VariantOrUnknownOrEmpty::Item(IntegerFormat::Int64),
                        ..Default::default()
                    })),
                }),
                "f64" => ReferenceOr::Item(Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::Number(Default::default())),
                }),
                "bool" => ReferenceOr::Item(Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::Boolean(Default::default())),
                }),
                _ => ReferenceOr::Reference {
                    reference: format!("#/components/schemas/{}", type_name),
                },
            };
        }
    }
    todo!()
}

use derive_more::{Display, From};

#[derive(Debug, Display, From)]
pub enum ToOpenApiError {
    Io(std::io::Error),
    Syn(syn::Error),
    Yaml(serde_yaml::Error),
}

impl std::error::Error for ToOpenApiError {}

pub fn generate<P: AsRef<Path>>(input: P, output: P) -> Result<(), ToOpenApiError> {
    let rust_code = fs::read_to_string(input)?;
    let mut ast = syn::parse_file(&rust_code)?;

    let mut visitor = Visitor {
        schemas: IndexMap::new(),
    };
    ast.items.sort_by_key(|i| match i {
        syn::Item::Struct(s) => s.ident.to_string(),
        _ => "".to_string(),
    });
    visitor.visit_file(&ast);

    let openapi = OpenAPI {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Test API".to_string(),
            version: "1.0.0".to_string(),
            ..Default::default()
        },
        paths: Paths::default(),
        components: Some(Components {
            schemas: visitor.schemas.into_iter().collect(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let spec = serde_yaml::to_string(&openapi)?;
    fs::write(output, spec)?;

    Ok(())
}

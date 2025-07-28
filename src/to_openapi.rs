use indexmap::IndexMap;
use openapiv3::{
    Components, Info, IntegerType, OpenAPI, Paths, ReferenceOr, Schema, SchemaKind, StringType,
    Type,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use syn::{
    visit::{self, Visit},
    ItemStruct, TypePath,
};

struct Visitor {
    schemas: BTreeMap<String, ReferenceOr<Schema>>,
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item_struct(&mut self, i: &'ast ItemStruct) {
        let mut properties = IndexMap::new();
        for field in &i.fields {
            if let Some(ident) = &field.ident {
                let ty = get_schema_from_type(&field.ty);
                properties.insert(ident.to_string(), ReferenceOr::Item(Box::new(ty)));
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

fn get_schema_from_type(ty: &syn::Type) -> Schema {
    if let syn::Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            let type_name = segment.ident.to_string();
            return match type_name.as_str() {
                "String" => Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::String(StringType::default())),
                },
                "i64" => Schema {
                    schema_data: Default::default(),
                    schema_kind: SchemaKind::Type(Type::Integer(IntegerType {
                        format: openapiv3::IntegerFormat::Int64,
                        ..Default::default()
                    })),
                },
                _ => todo!(),
            };
        }
    }
    todo!()
}

pub fn generate<P: AsRef<Path>>(input: P, output: P) -> Result<(), Box<dyn std::error::Error>> {
    let rust_code = fs::read_to_string(input)?;
    let ast = syn::parse_file(&rust_code)?;

    let mut visitor = Visitor {
        schemas: BTreeMap::new(),
    };
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

use openapiv3::{ObjectType, OpenAPI, SchemaKind, Type};
use std::fs;
use std::path::Path;

use heck::ToPascalCase;
use quote::quote;
use std::io::Write;

use derive_more::{Display, From};

#[derive(Debug, Display, From)]
pub enum FromOpenApiError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Syn(syn::Error),
}

impl std::error::Error for FromOpenApiError {}

pub fn generate<P: AsRef<Path>>(input: P, output: P) -> Result<(), FromOpenApiError> {
    let spec_path = input.as_ref();
    let output_path = output.as_ref();
    let spec = load_spec(spec_path)?;

    let mut models_file = fs::File::create(output_path.join("models.rs"))?;

    if let Some(components) = &spec.components {
        for (name, schema) in &components.schemas {
            let struct_name =
                syn::Ident::new(&name.to_pascal_case(), proc_macro2::Span::call_site());
            let schema = schema.as_item().unwrap();
            if let SchemaKind::Type(Type::Object(ObjectType { properties, .. })) =
                &schema.schema_kind
            {
                let mut fields = Vec::new();
                for (name, schema) in properties {
                    let field_name = syn::Ident::new(name, proc_macro2::Span::call_site());
                    let schema = schema.as_item().unwrap();
                    let field_type = match schema.schema_kind.clone() {
                        SchemaKind::Type(Type::String(_)) => quote! { String },
                        SchemaKind::Type(Type::Integer(_)) => quote! { i64 },
                        _ => todo!(),
                    };
                    fields.push(quote! { pub #field_name: #field_type });
                }

                let gen = quote! {
                    #[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
                    pub struct #struct_name {
                        #(#fields),*
                    }
                };
                let formatted = prettyplease::unparse(&syn::parse2(gen)?);
                write!(models_file, "{}", formatted)?;
            }
        }
    }

    Ok(())
}

fn load_spec<P: AsRef<Path>>(path: P) -> Result<OpenAPI, FromOpenApiError> {
    let s = fs::read_to_string(path)?;
    let spec: OpenAPI = serde_yaml::from_str(&s)?;
    Ok(spec)
}

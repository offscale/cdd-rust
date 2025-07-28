use openapiv3::{ObjectType, OpenAPI, SchemaKind, Type};
use std::fs;
use std::path::Path;

use heck::{AsSnakeCase, ToPascalCase};
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

pub fn generate<P: AsRef<Path>>(
    input: P,
    output: P,
    schema_output: P,
) -> Result<(), FromOpenApiError> {
    let spec_path = input.as_ref();
    let output_path = output.as_ref();
    let spec = load_spec(spec_path)?;

    let mut models_file = fs::File::create(output_path.join("models.rs"))?;
    let mut schema_file = fs::File::create(schema_output.as_ref().join("schema.rs"))?;

    if let Some(components) = &spec.components {
        for (name, schema) in &components.schemas {
            let struct_name =
                syn::Ident::new(&name.to_pascal_case(), proc_macro2::Span::call_site());
            let table_name = syn::Ident::new(
                &format!("{}", AsSnakeCase(name)),
                proc_macro2::Span::call_site(),
            );

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
                    #[derive(
                        Clone,
                        Debug,
                        PartialEq,
                        Queryable,
                        Insertable,
                        serde::Deserialize,
                        serde::Serialize,
                    )]
                    #[diesel(table_name = #table_name)]
                    pub struct #struct_name {
                        #(#fields),*
                    }
                };
                let formatted = prettyplease::unparse(&syn::parse2(gen)?);
                write!(models_file, "{}", formatted)?;

                let mut schema_fields = Vec::new();
                for (name, schema) in properties {
                    let field_name = syn::Ident::new(name, proc_macro2::Span::call_site());
                    let schema = schema.as_item().unwrap();
                    let field_type = match schema.schema_kind.clone() {
                        SchemaKind::Type(Type::String(_)) => quote! { Text },
                        SchemaKind::Type(Type::Integer(_)) => quote! { BigInt },
                        _ => todo!(),
                    };
                    schema_fields.push(quote! { #field_name -> #field_type, });
                }

                let schema_gen = quote! {
                    diesel::table! {
                        #table_name (id) {
                            #(#schema_fields)*
                        }
                    }
                };
                write!(schema_file, "{}", schema_gen.to_string())?;
            }
        }
    }

    Ok(())
}

pub fn generate_tests<P: AsRef<Path>>(
    input: P,
    output: P,
) -> Result<(), FromOpenApiError> {
    let spec_path = input.as_ref();
    let output_path = output.as_ref();
    let spec = load_spec(spec_path)?;

    let mut tests_file = fs::File::create(output_path.join("tests.rs"))?;

    for (path, path_item) in &spec.paths.paths {
        if let Some(get) = &path_item.as_item().unwrap().get {
            let operation_id = get.operation_id.as_ref().unwrap();
            let test_name = syn::Ident::new(&format!("test_{}", operation_id), proc_macro2::Span::call_site());
            let gen = quote! {
                #[actix_web::test]
                async fn #test_name() {
                    let req = actix_web::test::TestRequest::get().uri(#path).to_request();
                    let resp = actix_web::test::call_service(&app, req).await;
                    assert!(resp.status().is_success());
                }
            };
            let formatted = prettyplease::unparse(&syn::parse2(gen)?);
            write!(tests_file, "{}", formatted)?;
        }
    }

    Ok(())
}

fn load_spec<P: AsRef<Path>>(path: P) -> Result<OpenAPI, FromOpenApiError> {
    let s = fs::read_to_string(path)?;
    let spec: OpenAPI = serde_yaml::from_str(&s)?;
    Ok(spec)
}

pub fn generate_routes<P: AsRef<Path>>(
    input: P,
    output: P,
) -> Result<(), FromOpenApiError> {
    let spec_path = input.as_ref();
    let output_path = output.as_ref();
    let spec = load_spec(spec_path)?;

    let mut routes_file = fs::File::create(output_path.join("routes.rs"))?;

    for (path, path_item) in &spec.paths.paths {
        if let Some(get) = &path_item.as_item().unwrap().get {
            let operation_id = get.operation_id.as_ref().unwrap();
            let function_name = syn::Ident::new(operation_id, proc_macro2::Span::call_site());
            let gen = quote! {
                #[get(#path)]
                async fn #function_name() -> impl Responder {
                    HttpResponse::Ok().body("Hello world!")
                }
            };
            let formatted = prettyplease::unparse(&syn::parse2(gen)?);
            write!(routes_file, "{}", formatted)?;
        }
    }

    Ok(())
}

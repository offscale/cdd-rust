use crate::error::AppResult;
use crate::patcher::files::add_import;
use crate::patcher::structs::add_derive;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode, SourceFile};

/// Scans the entire file for structs and injects `#[derive(ToSchema)]` and the necessary import.
pub fn inject_openapi_attributes(source: &str) -> AppResult<String> {
    // 1. Add Import
    let mut current_source = add_import(source, "use utoipa::ToSchema;")?;

    // 2. Scan to find targets (names only)
    let struct_names = {
        let parse = SourceFile::parse(&current_source, Edition::Edition2021);
        let names: Vec<String> = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
            .filter_map(|s| s.name())
            .map(|n| n.text().to_string())
            .collect();
        names
    };

    // 3. Apply changes sequentially
    for name in struct_names {
        current_source = add_derive(&current_source, &name, "ToSchema")?;
    }

    Ok(current_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_openapi_attributes_full_flow() {
        let code = r#"
            struct User {
                id: i32
            }

            struct Post {
                title: String
            }
        "#;

        let res = inject_openapi_attributes(code).unwrap();

        assert!(res.contains("use utoipa::ToSchema;"));
        assert!(res.contains("#[derive(ToSchema)]\n            struct User"));
        assert!(res.contains("#[derive(ToSchema)]\n            struct Post"));
    }

    #[test]
    fn test_inject_openapi_keeps_existing() {
        let code = r#"
            use std::collections::HashMap;

            #[derive(Debug)]
            struct User { id: i32 }
        "#;

        let res = inject_openapi_attributes(code).unwrap();

        assert!(res.contains("use std::collections::HashMap;"));
        assert!(res.contains("use utoipa::ToSchema;"));
        assert!(res.contains("#[derive(Debug, ToSchema)]"));
    }
}

use std::collections::HashMap;
use syn::visit::Visit;
use syn::Expr;
use syn::Field;
use syn::TypePath;

/// try to parse the actix server resource path ant the function
/// pattern to watch out for: use actix_web::server
/// server::new(||.. app... resource("/urls".. method.(fn_call)
///
/// Note: make sure actix_web::server is not renamed, or we can be able to recognize it.
/// Note: resouse and app state, middle configuration might also be located
/// in different function call, might need to search for it first
pub struct Actix {
    fn_calls: HashMap<String, String>,
}

impl Actix {
    pub fn new() -> Self {
        Actix {
            fn_calls: HashMap::new(),
        }
    }

    fn parse_expr_path(path: &syn::ExprPath) -> String {
        //println!("expr path: {:#?}", path);
        //println!("path is: {:#?}", path.path);
        for segment in &path.path.segments {
            //println!("segment: {:#?}", segment);
            //println!("ident: {}", segment.ident.to_string());
        }
        let fn_call = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<String>>()
            .join("::");
        println!("fn call: {}", fn_call);
        fn_call
    }

    /// look for the server::new pattern
    fn parse_actix_server(expr: &Box<syn::Expr>) {
        match expr.as_ref() {
            Expr::Path(path) => {
                let fn_call = Self::parse_expr_path(path);
            }
            _ => (),
        }
    }
}

impl<'ast> Visit<'ast> for Actix {
    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        //println!("this is an expr call: {:#?}", c);
        Self::parse_actix_server(&c.func);
    }
}

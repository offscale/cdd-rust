#![deny(missing_docs)]

//! # Arazzo 1.1.0 Runtime Expression AST & Rust Code Generator
//!
//! Classifies all 17 Arazzo 1.1.0 runtime expression roots into a strongly typed AST
//! and provides idiomatic Rust expression emitters for SDK, CLI, and Server targets.

use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::{
    split_runtime_expression_template, RuntimeExpression, RuntimeExpressionSegment,
};
use crate::openapi::parse::routes::naming::to_snake_case;

/// Source sub-target for `$request.*` and `$response.*` runtime expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMessageSource {
    /// HTTP header token (`header.<token>`).
    Header(String),
    /// URL query parameter (`query.<name>`).
    Query(String),
    /// URL path parameter (`path.<name>`).
    Path(String),
    /// Message body with optional RFC 6901 JSON Pointer (`body` or `body#/ptr`).
    Body(Option<String>),
}

/// Source sub-target for AsyncAPI `$message.*` runtime expressions (Arazzo 1.1.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncMessageSource {
    /// Message header token (`header.<token>`).
    HeaderToken(String),
    /// Message header with optional RFC 6901 JSON Pointer (`header` or `header#/ptr`).
    HeaderPointer(Option<String>),
    /// Message payload with optional RFC 6901 JSON Pointer (`payload` or `payload#/ptr`).
    Payload(Option<String>),
}

/// Target section of a `$components.*` runtime expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSection {
    /// `$components.inputs.<name>`
    Inputs,
    /// `$components.parameters.<name>`
    Parameters,
    /// `$components.successActions.<name>`
    SuccessActions,
    /// `$components.failureActions.<name>`
    FailureActions,
}

/// Strongly typed AST classification for all 17 Arazzo 1.1.0 Runtime Expression roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArazzoExpressionRoot {
    /// `$url`
    Url,
    /// `$method`
    Method,
    /// `$statusCode`
    StatusCode,
    /// `$self` (Arazzo 1.1.0 canonical document URI)
    SelfUri,
    /// `$request.<source>`
    Request(HttpMessageSource),
    /// `$response.<source>`
    Response(HttpMessageSource),
    /// `$message.<source>` (Arazzo 1.1.0 AsyncAPI)
    Message(AsyncMessageSource),
    /// `$inputs.<name>` with optional JSON Pointer (`#/ptr`)
    Inputs {
        /// Property path (e.g. `"userId"` or `"user.id"`).
        name: String,
        /// Optional RFC 6901 JSON Pointer (`"/id"`).
        pointer: Option<String>,
    },
    /// `$outputs.<name>` with optional JSON Pointer (`#/ptr`)
    Outputs {
        /// Output key name.
        name: String,
        /// Optional RFC 6901 JSON Pointer.
        pointer: Option<String>,
    },
    /// `$steps.<stepId>.<member>` with optional JSON Pointer (`#/ptr`)
    Steps {
        /// Step identifier.
        step_id: String,
        /// Member path (e.g. `"outputs.token"`).
        member: String,
        /// Optional RFC 6901 JSON Pointer.
        pointer: Option<String>,
    },
    /// `$workflows.<workflowId>.<member>` with optional JSON Pointer (`#/ptr`)
    Workflows {
        /// Workflow identifier.
        workflow_id: String,
        /// Member path (e.g. `"outputs.jwt"` or `"inputs.user"`).
        member: String,
        /// Optional RFC 6901 JSON Pointer.
        pointer: Option<String>,
    },
    /// `$sourceDescriptions.<name>.<member>` with optional JSON Pointer (`#/ptr`)
    SourceDescriptions {
        /// Source description name.
        name: String,
        /// Member path (typically `"url"` or an operation identifier).
        member: String,
        /// Optional RFC 6901 JSON Pointer.
        pointer: Option<String>,
    },
    /// `$components.<section>.<name>`
    Components {
        /// Component registry section.
        section: ComponentSection,
        /// Component key name.
        name: String,
    },
}

impl ArazzoExpressionRoot {
    /// Parses a single `$` runtime expression into its strongly typed [`ArazzoExpressionRoot`].
    ///
    /// # Arguments
    /// * `raw` - Raw runtime expression string (with or without surrounding `{}`).
    ///
    /// # Returns
    /// A classified [`ArazzoExpressionRoot`].
    ///
    /// # Errors
    /// Returns [`AppError::RuntimeExpressionParse`] if the expression does not match the Arazzo 1.1.0 ABNF.
    pub fn parse(raw: &str) -> AppResult<Self> {
        let validated = RuntimeExpression::parse_expression(raw).map_err(|e| {
            AppError::RuntimeExpressionParse {
                expression: raw.to_string(),
                reason: e.to_string(),
            }
        })?;
        let s = validated.as_str();
        match s {
            "$url" => return Ok(Self::Url),
            "$method" => return Ok(Self::Method),
            "$statusCode" => return Ok(Self::StatusCode),
            "$self" => return Ok(Self::SelfUri),
            _ => {}
        }

        if let Some(rest) = s.strip_prefix("$request.") {
            return Ok(Self::Request(parse_http_source(rest)));
        }
        if let Some(rest) = s.strip_prefix("$response.") {
            return Ok(Self::Response(parse_http_source(rest)));
        }
        if let Some(rest) = s.strip_prefix("$message.") {
            return Ok(Self::Message(parse_async_source(rest)));
        }
        if let Some(rest) = s.strip_prefix("$inputs.") {
            let (name, pointer) = split_pointer(rest);
            return Ok(Self::Inputs { name, pointer });
        }
        if let Some(rest) = s.strip_prefix("$outputs.") {
            let (name, pointer) = split_pointer(rest);
            return Ok(Self::Outputs { name, pointer });
        }
        if let Some(rest) = s.strip_prefix("$steps.") {
            let (step_id, tail) = rest.split_once('.').unwrap_or((rest, ""));
            let (member, pointer) = split_pointer(tail);
            return Ok(Self::Steps {
                step_id: step_id.to_string(),
                member,
                pointer,
            });
        }
        if let Some(rest) = s.strip_prefix("$workflows.") {
            let (workflow_id, tail) = rest.split_once('.').unwrap_or((rest, ""));
            let (member, pointer) = split_pointer(tail);
            return Ok(Self::Workflows {
                workflow_id: workflow_id.to_string(),
                member,
                pointer,
            });
        }
        if let Some(rest) = s.strip_prefix("$sourceDescriptions.") {
            let (name, tail) = rest.split_once('.').unwrap_or((rest, ""));
            let (member, pointer) = split_pointer(tail);
            return Ok(Self::SourceDescriptions {
                name: name.to_string(),
                member,
                pointer,
            });
        }
        let rest = s.strip_prefix("$components.").unwrap_or(s);
        let (sec, name) = rest.split_once('.').unwrap_or((rest, ""));
        let section = match sec {
            "inputs" => ComponentSection::Inputs,
            "parameters" => ComponentSection::Parameters,
            "successActions" => ComponentSection::SuccessActions,
            _ => ComponentSection::FailureActions,
        };
        Ok(Self::Components {
            section,
            name: name.to_string(),
        })
    }

    /// Generates an idiomatic Rust evaluation expression for use in generated workflow methods.
    pub fn to_rust_expr(&self) -> String {
        match self {
            Self::Url => "req_url.clone()".to_string(),
            Self::Method => "req_method.clone()".to_string(),
            Self::StatusCode => "status_code".to_string(),
            Self::SelfUri => "self_uri.clone()".to_string(),
            Self::Request(HttpMessageSource::Header(h)) => {
                format!("req_headers.get({:?}).cloned().unwrap_or_default()", h)
            }
            Self::Request(HttpMessageSource::Query(q)) => {
                format!("req_query.get({:?}).cloned().unwrap_or_default()", q)
            }
            Self::Request(HttpMessageSource::Path(p)) => {
                format!("req_path.get({:?}).cloned().unwrap_or_default()", p)
            }
            Self::Request(HttpMessageSource::Body(Some(ptr))) => {
                format!("req_body.pointer({:?}).cloned().unwrap_or_default()", ptr)
            }
            Self::Request(HttpMessageSource::Body(None)) => "req_body.clone()".to_string(),
            Self::Response(HttpMessageSource::Header(h)) => {
                format!("resp_headers.get({:?}).cloned().unwrap_or_default()", h)
            }
            Self::Response(HttpMessageSource::Query(q)) => {
                format!("resp_query.get({:?}).cloned().unwrap_or_default()", q)
            }
            Self::Response(HttpMessageSource::Path(p)) => {
                format!("resp_path.get({:?}).cloned().unwrap_or_default()", p)
            }
            Self::Response(HttpMessageSource::Body(Some(ptr))) => {
                format!("resp_body.pointer({:?}).cloned().unwrap_or_default()", ptr)
            }
            Self::Response(HttpMessageSource::Body(None)) => "resp_body.clone()".to_string(),
            Self::Message(AsyncMessageSource::HeaderToken(h)) => {
                format!("msg_headers.get({:?}).cloned().unwrap_or_default()", h)
            }
            Self::Message(AsyncMessageSource::HeaderPointer(Some(ptr))) => {
                format!(
                    "msg_header_json.pointer({:?}).cloned().unwrap_or_default()",
                    ptr
                )
            }
            Self::Message(AsyncMessageSource::HeaderPointer(None)) => {
                "msg_header_json.clone()".to_string()
            }
            Self::Message(AsyncMessageSource::Payload(Some(ptr))) => {
                format!(
                    "msg_payload.pointer({:?}).cloned().unwrap_or_default()",
                    ptr
                )
            }
            Self::Message(AsyncMessageSource::Payload(None)) => "msg_payload.clone()".to_string(),
            Self::Inputs { name, pointer } => {
                let field = to_snake_case(&name.replace('.', "_"));
                match pointer {
                    Some(ptr) => format!(
                        "inputs.{}.pointer({:?}).cloned().unwrap_or_default()",
                        field, ptr
                    ),
                    None => format!("inputs.{}.clone()", field),
                }
            }
            Self::Outputs { name, pointer } => {
                let field = to_snake_case(&name.replace('.', "_"));
                match pointer {
                    Some(ptr) => format!(
                        "outputs.{}.pointer({:?}).cloned().unwrap_or_default()",
                        field, ptr
                    ),
                    None => format!("outputs.{}.clone()", field),
                }
            }
            Self::Steps {
                step_id,
                member,
                pointer,
            } => {
                let step_var = format!("step_{}_outputs", to_snake_case(step_id));
                let out_name = member.strip_prefix("outputs.").unwrap_or(member);
                let field = to_snake_case(&out_name.replace('.', "_"));
                match pointer {
                    Some(ptr) => format!(
                        "{}.{}.pointer({:?}).cloned().unwrap_or_default()",
                        step_var, field, ptr
                    ),
                    None => format!("{}.{}.clone()", step_var, field),
                }
            }
            Self::Workflows {
                workflow_id,
                member,
                pointer,
            } => {
                let wf_var = format!("wf_{}_outputs", to_snake_case(workflow_id));
                let out_name = member.strip_prefix("outputs.").unwrap_or(member);
                let field = to_snake_case(&out_name.replace('.', "_"));
                match pointer {
                    Some(ptr) => format!(
                        "{}.{}.pointer({:?}).cloned().unwrap_or_default()",
                        wf_var, field, ptr
                    ),
                    None => format!("{}.{}.clone()", wf_var, field),
                }
            }
            Self::SourceDescriptions {
                name,
                member,
                pointer: _,
            } => {
                let src_var = to_snake_case(name);
                if member == "url" {
                    format!("self.{}_base_url.clone()", src_var)
                } else {
                    format!("self.{}_{}.clone()", src_var, to_snake_case(member))
                }
            }
            Self::Components { section, name } => {
                let prefix = match section {
                    ComponentSection::Inputs => "component_input",
                    ComponentSection::Parameters => "component_param",
                    ComponentSection::SuccessActions => "component_success_action",
                    ComponentSection::FailureActions => "component_failure_action",
                };
                format!("{}_{}()", prefix, to_snake_case(name))
            }
        }
    }
}

/// Converts a [`RuntimeExpression`] (either a direct `$` expression or a template string containing `{...}`)
/// into an idiomatic Rust expression string.
///
/// # Arguments
/// * `expr` - The validated [`RuntimeExpression`].
///
/// # Returns
/// A Rust code snippet evaluating the runtime expression or `format!(...)` template.
///
/// # Errors
/// Returns [`AppError::RuntimeExpressionParse`] if any embedded expression is invalid.
pub fn runtime_expression_to_rust(expr: &RuntimeExpression) -> AppResult<String> {
    let raw = expr.as_str();
    if raw.starts_with('$') {
        return Ok(ArazzoExpressionRoot::parse(raw)?.to_rust_expr());
    }

    let segments = split_runtime_expression_template(raw);
    let mut fmt_str = String::new();
    let mut args = Vec::new();
    for seg in segments {
        match seg {
            RuntimeExpressionSegment::Literal(lit) => {
                fmt_str.push_str(&lit.replace('{', "{{").replace('}', "}}"));
            }
            RuntimeExpressionSegment::Expression(inner) => {
                fmt_str.push_str("{}");
                args.push(ArazzoExpressionRoot::parse(&inner)?.to_rust_expr());
            }
        }
    }

    if args.is_empty() {
        Ok(format!("{:?}.to_string()", raw))
    } else {
        Ok(format!("format!({:?}, {})", fmt_str, args.join(", ")))
    }
}

fn split_pointer(tail: &str) -> (String, Option<String>) {
    match tail.split_once('#') {
        Some((head, ptr)) => {
            let normalized_ptr = if ptr.is_empty() {
                None
            } else {
                Some(ptr.to_string())
            };
            (head.to_string(), normalized_ptr)
        }
        None => (tail.to_string(), None),
    }
}

fn parse_http_source(rest: &str) -> HttpMessageSource {
    if let Some(h) = rest.strip_prefix("header.") {
        return HttpMessageSource::Header(h.to_string());
    }
    if let Some(q) = rest.strip_prefix("query.") {
        return HttpMessageSource::Query(q.to_string());
    }
    if let Some(p) = rest.strip_prefix("path.") {
        return HttpMessageSource::Path(p.to_string());
    }
    let b = rest.strip_prefix("body").unwrap_or(rest);
    let (_, ptr) = split_pointer(b);
    HttpMessageSource::Body(ptr)
}

fn parse_async_source(rest: &str) -> AsyncMessageSource {
    if let Some(h) = rest.strip_prefix("header.") {
        return AsyncMessageSource::HeaderToken(h.to_string());
    }
    if let Some(h) = rest.strip_prefix("header") {
        let (_, ptr) = split_pointer(h);
        return AsyncMessageSource::HeaderPointer(ptr);
    }
    let p = rest.strip_prefix("payload").unwrap_or(rest);
    let (_, ptr) = split_pointer(p);
    AsyncMessageSource::Payload(ptr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_17_arazzo_expression_roots_and_rust_codegen() {
        let cases = [
            ("$url", ArazzoExpressionRoot::Url, "req_url.clone()"),
            (
                "$method",
                ArazzoExpressionRoot::Method,
                "req_method.clone()",
            ),
            (
                "$statusCode",
                ArazzoExpressionRoot::StatusCode,
                "status_code",
            ),
            ("$self", ArazzoExpressionRoot::SelfUri, "self_uri.clone()"),
            (
                "$request.header.X-Token",
                ArazzoExpressionRoot::Request(HttpMessageSource::Header("X-Token".to_string())),
                r#"req_headers.get("X-Token").cloned().unwrap_or_default()"#,
            ),
            (
                "$request.query.filter",
                ArazzoExpressionRoot::Request(HttpMessageSource::Query("filter".to_string())),
                r#"req_query.get("filter").cloned().unwrap_or_default()"#,
            ),
            (
                "$request.path.id",
                ArazzoExpressionRoot::Request(HttpMessageSource::Path("id".to_string())),
                r#"req_path.get("id").cloned().unwrap_or_default()"#,
            ),
            (
                "$request.body",
                ArazzoExpressionRoot::Request(HttpMessageSource::Body(None)),
                "req_body.clone()",
            ),
            (
                "$request.body#",
                ArazzoExpressionRoot::Request(HttpMessageSource::Body(None)),
                "req_body.clone()",
            ),
            (
                "$request.body#/user/id",
                ArazzoExpressionRoot::Request(HttpMessageSource::Body(Some(
                    "/user/id".to_string(),
                ))),
                r#"req_body.pointer("/user/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$response.header.Location",
                ArazzoExpressionRoot::Response(HttpMessageSource::Header("Location".to_string())),
                r#"resp_headers.get("Location").cloned().unwrap_or_default()"#,
            ),
            (
                "$response.query.page",
                ArazzoExpressionRoot::Response(HttpMessageSource::Query("page".to_string())),
                r#"resp_query.get("page").cloned().unwrap_or_default()"#,
            ),
            (
                "$response.path.id",
                ArazzoExpressionRoot::Response(HttpMessageSource::Path("id".to_string())),
                r#"resp_path.get("id").cloned().unwrap_or_default()"#,
            ),
            (
                "$response.body",
                ArazzoExpressionRoot::Response(HttpMessageSource::Body(None)),
                "resp_body.clone()",
            ),
            (
                "$response.body#/id",
                ArazzoExpressionRoot::Response(HttpMessageSource::Body(Some("/id".to_string()))),
                r#"resp_body.pointer("/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$message.header.X-Corr",
                ArazzoExpressionRoot::Message(AsyncMessageSource::HeaderToken(
                    "X-Corr".to_string(),
                )),
                r#"msg_headers.get("X-Corr").cloned().unwrap_or_default()"#,
            ),
            (
                "$message.header",
                ArazzoExpressionRoot::Message(AsyncMessageSource::HeaderPointer(None)),
                "msg_header_json.clone()",
            ),
            (
                "$message.header#/corr",
                ArazzoExpressionRoot::Message(AsyncMessageSource::HeaderPointer(Some(
                    "/corr".to_string(),
                ))),
                r#"msg_header_json.pointer("/corr").cloned().unwrap_or_default()"#,
            ),
            (
                "$message.payload",
                ArazzoExpressionRoot::Message(AsyncMessageSource::Payload(None)),
                "msg_payload.clone()",
            ),
            (
                "$message.payload#/event/id",
                ArazzoExpressionRoot::Message(AsyncMessageSource::Payload(Some(
                    "/event/id".to_string(),
                ))),
                r#"msg_payload.pointer("/event/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$inputs.petId",
                ArazzoExpressionRoot::Inputs {
                    name: "petId".to_string(),
                    pointer: None,
                },
                "inputs.pet_id.clone()",
            ),
            (
                "$inputs.pet#/id",
                ArazzoExpressionRoot::Inputs {
                    name: "pet".to_string(),
                    pointer: Some("/id".to_string()),
                },
                r#"inputs.pet.pointer("/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$outputs.orderId",
                ArazzoExpressionRoot::Outputs {
                    name: "orderId".to_string(),
                    pointer: None,
                },
                "outputs.order_id.clone()",
            ),
            (
                "$outputs.order#/id",
                ArazzoExpressionRoot::Outputs {
                    name: "order".to_string(),
                    pointer: Some("/id".to_string()),
                },
                r#"outputs.order.pointer("/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$steps.loginStep.outputs.accessToken",
                ArazzoExpressionRoot::Steps {
                    step_id: "loginStep".to_string(),
                    member: "outputs.accessToken".to_string(),
                    pointer: None,
                },
                "step_login_step_outputs.access_token.clone()",
            ),
            (
                "$steps.loginStep.outputs.user#/id",
                ArazzoExpressionRoot::Steps {
                    step_id: "loginStep".to_string(),
                    member: "outputs.user".to_string(),
                    pointer: Some("/id".to_string()),
                },
                r#"step_login_step_outputs.user.pointer("/id").cloned().unwrap_or_default()"#,
            ),
            (
                "$workflows.authFlow.outputs.jwt",
                ArazzoExpressionRoot::Workflows {
                    workflow_id: "authFlow".to_string(),
                    member: "outputs.jwt".to_string(),
                    pointer: None,
                },
                "wf_auth_flow_outputs.jwt.clone()",
            ),
            (
                "$workflows.authFlow.outputs.token#/raw",
                ArazzoExpressionRoot::Workflows {
                    workflow_id: "authFlow".to_string(),
                    member: "outputs.token".to_string(),
                    pointer: Some("/raw".to_string()),
                },
                r#"wf_auth_flow_outputs.token.pointer("/raw").cloned().unwrap_or_default()"#,
            ),
            (
                "$sourceDescriptions.petstore.url",
                ArazzoExpressionRoot::SourceDescriptions {
                    name: "petstore".to_string(),
                    member: "url".to_string(),
                    pointer: None,
                },
                "self.petstore_base_url.clone()",
            ),
            (
                "$sourceDescriptions.petstore.getPet",
                ArazzoExpressionRoot::SourceDescriptions {
                    name: "petstore".to_string(),
                    member: "getPet".to_string(),
                    pointer: None,
                },
                "self.petstore_get_pet.clone()",
            ),
            (
                "$components.inputs.UserSchema",
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::Inputs,
                    name: "UserSchema".to_string(),
                },
                "component_input_user_schema()",
            ),
            (
                "$components.parameters.AuthHeader",
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::Parameters,
                    name: "AuthHeader".to_string(),
                },
                "component_param_auth_header()",
            ),
            (
                "$components.successActions.DoneAction",
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::SuccessActions,
                    name: "DoneAction".to_string(),
                },
                "component_success_action_done_action()",
            ),
            (
                "$components.failureActions.RetryAction",
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::FailureActions,
                    name: "RetryAction".to_string(),
                },
                "component_failure_action_retry_action()",
            ),
        ];

        for (raw, expected_ast, expected_rust) in cases {
            let parsed = ArazzoExpressionRoot::parse(raw).expect(raw);
            assert_eq!(parsed, expected_ast);
            assert_eq!(parsed.to_rust_expr(), expected_rust);
        }

        assert!(ArazzoExpressionRoot::parse("$invalid").is_err());
    }

    #[test]
    fn test_runtime_expression_to_rust_templates_and_literals() {
        let direct = RuntimeExpression::parse("$inputs.userId").expect("direct");
        assert_eq!(
            runtime_expression_to_rust(&direct).expect("rust"),
            "inputs.user_id.clone()"
        );

        let template =
            RuntimeExpression::parse("Bearer {$steps.login.outputs.token}").expect("template");
        assert_eq!(
            runtime_expression_to_rust(&template).expect("rust"),
            r#"format!("Bearer {}", step_login_outputs.token.clone())"#
        );

        let literal = RuntimeExpression::parse("plain_literal").expect("literal");
        assert_eq!(
            runtime_expression_to_rust(&literal).expect("rust"),
            r#""plain_literal".to_string()"#
        );
    }
}

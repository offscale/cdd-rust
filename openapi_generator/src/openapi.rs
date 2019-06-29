use colored::Colorize;
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    ffi::OsStr,
    fmt::{
        self,
        Display,
    },
    fs,
    io::{
        self,
        Write,
    },
    ops::Not,
    path::{
        Path,
        PathBuf,
    },
    process,
};
#[derive(Debug, Serialize)]
pub struct OpenApiDocument {
    pub swagger: String,
    pub info: OpenApiInfo,
    pub paths: Vec<()>,
    pub components: Vec<OpenApiComponents>,
}

#[derive(Debug, Serialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct OpenApiComponents {
    pub schemas: HashMap<String, OpenApiSchema>,
}

#[derive(Debug, Serialize)]
pub struct OpenApiSchema {
    pub required: Vec<String>,
    pub properties: HashMap<String, OpenApiProperties>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenApiProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<OpenApiProperties>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub o_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "$ref")]
    pub dollar_ref: Option<String>,
    #[serde(skip_serializing_if = "Not::not")]
    #[serde(rename = "uniqueItems")]
    pub unique_items: bool,
}

impl OpenApiProperties {
    pub fn new(o_type: String) -> OpenApiProperties {
        OpenApiProperties {
            dollar_ref: None,
            format: None,
            items: None,
            minimum: None,
            o_type: Some(o_type),
            unique_items: false,
        }
    }

    pub fn new_ref(dollar_ref: String) -> OpenApiProperties {
        OpenApiProperties {
            dollar_ref: Some(dollar_ref),
            format: None,
            items: None,
            minimum: None,
            o_type: None,
            unique_items: false,
        }
    }

    pub fn with_minimum(&self, minimum: i64) -> Self {
        let mut new = self.clone();
        new.minimum = Some(minimum);
        new
    }

    pub fn with_format(&self, format: String) -> Self {
        let mut new = self.clone();
        new.format = Some(format);
        new
    }

    pub fn with_items(&self, items: OpenApiProperties) -> Self {
        let mut new = self.clone();
        new.items = Some(Box::new(items));
        new
    }

    pub fn with_unique_items(&self) -> Self {
        let mut new = self.clone();
        new.unique_items = true;
        new
    }
}

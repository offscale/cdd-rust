const fs = require('fs');
let code = fs.readFileSync('models.rs', 'utf8');

code = code.replace(
  '#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub enum ParamSource {\n    /// Path.\n    Path,\n    /// Query.\n    Query,\n    /// Query String (OAS 3.2).\n    QueryString,\n    /// Header.\n    Header,\n    /// Cookie.\n    Cookie,\n}',
  '#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase")]\npub enum ParamSource {\n    /// Path.\n    Path,\n    /// Query.\n    Query,\n    /// Query String (OAS 3.2).\n    QueryString,\n    /// Header.\n    Header,\n    /// Cookie.\n    Cookie,\n    /// Form Data.\n    #[serde(rename = "formData")]\n    FormData,\n    /// Body.\n    #[serde(rename = "body")]\n    Body,\n}'
);

fs.writeFileSync('models.rs', code);

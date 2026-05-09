const fs = require('fs');
let code = fs.readFileSync('models.rs', 'utf8');

code = code.replace(
  '#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub enum ParamSource {',
  '#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = "camelCase")]\npub enum ParamSource {'
);

fs.writeFileSync('models.rs', code);

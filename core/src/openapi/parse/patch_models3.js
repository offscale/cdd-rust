const fs = require('fs');
let code = fs.readFileSync('models.rs', 'utf8');

code = code.replace('    #[serde(rename = "formData")]\n    FormData,', '    /// Form data.\n    #[serde(rename = "formData")]\n    FormData,');
code = code.replace('    #[serde(rename = "body")]\n    Body,', '    /// Body.\n    #[serde(rename = "body")]\n    Body,');

fs.writeFileSync('models.rs', code);

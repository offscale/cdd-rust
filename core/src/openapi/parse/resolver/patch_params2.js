const fs = require('fs');
let code = fs.readFileSync('params.rs', 'utf8');

code = code.replace(
  '        "cookie" => ParamSource::Cookie,\n        _ => ParamSource::Query,',
  '        "cookie" => ParamSource::Cookie,\n        "formData" => ParamSource::FormData,\n        "body" => ParamSource::Body,\n        _ => ParamSource::Query,'
);

fs.writeFileSync('params.rs', code);

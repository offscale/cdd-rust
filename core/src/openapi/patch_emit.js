const fs = require('fs');
let code = fs.readFileSync('emit.rs', 'utf8');

code = code.replace(
  'ParamSource::Cookie => "cookie",',
  'ParamSource::Cookie => "cookie",\n        ParamSource::FormData => "formData",\n        ParamSource::Body => "body",'
);

fs.writeFileSync('emit.rs', code);

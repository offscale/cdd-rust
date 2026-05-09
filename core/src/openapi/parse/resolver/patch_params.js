const fs = require('fs');
let code = fs.readFileSync('params.rs', 'utf8');

code = code.replace(
  'ParamSource::Path | ParamSource::Header => Some(ParamStyle::Simple),',
  'ParamSource::Path | ParamSource::Header => Some(ParamStyle::Simple),\n                ParamSource::FormData | ParamSource::Body => None,'
);

code = code.replace(
  'ParamSource::Cookie => Some(ParamStyle::Form),',
  'ParamSource::Cookie => Some(ParamStyle::Form),\n        ParamSource::FormData | ParamSource::Body => None,'
);

code = code.replace(
  'ParamSource::Cookie => matches!(style, ParamStyle::Form | ParamStyle::Cookie),',
  'ParamSource::Cookie => matches!(style, ParamStyle::Form | ParamStyle::Cookie),\n        ParamSource::FormData | ParamSource::Body => false,'
);

code = code.replace(
  'ParamSource::Cookie => &["form", "cookie"],',
  'ParamSource::Cookie => &["form", "cookie"],\n        ParamSource::FormData | ParamSource::Body => &[],'
);

fs.writeFileSync('params.rs', code);

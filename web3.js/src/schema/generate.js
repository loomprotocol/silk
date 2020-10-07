const $path = require("path");
const $fs = require("fs");
var Ajv = require('ajv'); 
var pack = require('ajv-pack');
const schema = require('./solana');
const { compile } = require('json-schema-to-typescript');

const capitalize = (s) => {
    if (typeof s !== 'string') return ''
    return s.charAt(0).toUpperCase() + s.slice(1)
}

function toSafeString(string) {
    return capitalize(
    string
        // replace chars which are not valid for typescript identifiers with whitespace
        .replace(/(^\s*[^a-zA-Z_$])|([^a-zA-Z_$\d])/g, ' ')
        // uppercase leading underscores followed by lowercase
        .replace(/^_[a-z]/g, function (match) { return match.toUpperCase(); })
        // remove non-leading underscores followed by lowercase (convert snake_case)
        .replace(/_[a-z]/g, function (match) { return match.substr(1, match.length).toUpperCase(); })
        // uppercase letters after digits, dollars
        .replace(/([\d$]+[a-zA-Z])/g, function (match) { return match.toUpperCase(); })
        // uppercase first letter after whitespace
        .replace(/\s+([a-zA-Z])/g, function (match) { return match.toUpperCase().trim(); })
        // remove remaining whitespace
        .replace(/\s/g, ''));
}

var ajv = new Ajv({sourceCode: true});
const dir = $path.join(__dirname, `/validators`);
if(!$fs.existsSync(dir)) {
    $fs.mkdirSync(dir);
} 

const filesToImport = {};
ajv.addSchema(schema);

schema.forEach(s => {
    const id = s['$id'];
    const fileName = `./validators/${id}.js`;
    const functionName = id;
    const mainType = `${toSafeString(id)}${s.definitions ? '1': ''}`;
    filesToImport[functionName] = `./${id}`;

    // set to tru to precomple validation code -> generate larger bundle
    const PRECOMPILE = false;
    const validateCode = PRECOMPILE ? `${pack(ajv, ajv.getSchema(s['$id']))}` : `
import Ajv from 'ajv';
const ajv = new Ajv();
const validate  = ajv.compile(${JSON.stringify(s)})`;

    compile(s, id).then(ts => {
        var moduleCode = 
`// This file is autogenerated by ${__filename.replace(__dirname, 'build')}, do not edit
${validateCode}

${ts.split('[k: string]: unknown;').join('')}

function isError(obj: ${mainType})
// : obj is  Error 
{
    return obj.error !== undefined && obj.error !== null;
}

export const ${id} = {
    validate, 
    get: ((obj: unknown) => {
        if(!validate(obj)) {
            throw new Error(JSON.stringify(validate.errors))
        };

        return obj;
        // as ${mainType};
    }),
    isError
};
`.replace('module.exports = validate;', 'export { validate };');
    $fs.writeFileSync($path.join(__dirname, fileName), moduleCode);
        
    });
});

const imports = Object.keys(filesToImport).reduce((acc, key) => {
    const loc = filesToImport[key];
    const line = `export { ${key} } from '${filesToImport[key]}';\r\n`;
    return acc = acc + line;
}, '');

const code = `
${imports}
`;

$fs.writeFileSync($path.join(__dirname, './validators/index.js'), code);



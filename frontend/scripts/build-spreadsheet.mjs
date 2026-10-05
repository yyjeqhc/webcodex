import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (process.argv.slice(2).some(argument => argument !== '--check')) throw new Error('Only --check is supported');
const path = resolve(root, '../src/mcp_spreadsheet_app.html');
const library = readFileSync(resolve(root, 'vendor/sheetjs/xlsx.mini.min.js'), 'utf8');
if (createHash('sha256').update(library).digest('hex') !== '0cb353f830d7288385492c83d277b058ddeac664ca51cf1393aa1fd3e2b70939') throw new Error('SheetJS CE 0.20.3 vendor hash changed');
const license = readFileSync(resolve(root, 'vendor/sheetjs/LICENSE'), 'utf8');
const data = readFileSync(resolve(root, 'src/mcp-apps/spreadsheet-data.js'), 'utf8');
const app = readFileSync(resolve(root, 'src/mcp-apps/spreadsheet-app.js'), 'utf8');
const worker = `/* SheetJS CE 0.20.3 · https://sheetjs.com/\n${license.replace(/\*\//g, '* /')}\n*/\n${library}\n${data}\nself.onmessage = async event => { try { self.postMessage({workbook:await WebCodexSpreadsheet.parse(new Uint8Array(event.data.bytes),event.data.filename)}); } catch(error) { self.postMessage({error:String(error.message || error).slice(0,512)}); } };\n`;
let html = readFileSync(path, 'utf8');
for (const [name, source] of [['WORKER', worker], ['APP', data + '\n' + app]]) {
  const begin = `/* BEGIN GENERATED SPREADSHEET ${name} */`, end = `/* END GENERATED SPREADSHEET ${name} */`;
  if (html.split(begin).length !== 2 || html.split(end).length !== 2) throw new Error('Expected exactly one generated region: ' + name);
  const start = html.indexOf(begin) + begin.length, finish = html.indexOf(end);
  if (finish <= start) throw new Error('Invalid generated region: ' + name);
  html = html.slice(0, start) + '\n' + source.replace(/<\/script/gi, '<\\/script') + '\n' + html.slice(finish);
}
if (process.argv.includes('--check')) {
  if (readFileSync(path, 'utf8') !== html) throw new Error('Spreadsheet bundle is out of date; run npm --prefix frontend run build:spreadsheet');
  console.log('Spreadsheet bundle checked');
} else {
  writeFileSync(path, html);
  console.log(`Spreadsheet bundle generated (${Buffer.byteLength(html)} bytes; parser SHA-256 ${createHash('sha256').update(library).digest('hex')})`);
}

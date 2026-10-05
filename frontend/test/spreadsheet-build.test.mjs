import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { copyFile, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const vendor = 'frontend/vendor/sheetjs/xlsx.mini.min.js';
const inputs = [
  'frontend/vendor/sheetjs/LICENSE',
  'frontend/scripts/build-spreadsheet.mjs',
  'frontend/src/mcp-apps/spreadsheet-data.js',
  'frontend/src/mcp-apps/spreadsheet-app.js',
  'src/mcp_spreadsheet_app.html',
];

test('Git recheckout preserves spreadsheet vendor bytes and bundle checks with Windows autocrlf', async t => {
  const temporaryRoot = await realpath(tmpdir());
  const fixture = await mkdtemp(join(temporaryRoot, 'webcodex-spreadsheet-checkout-'));
  t.after(async () => {
    // Only remove the disposable directory created by this test, after resolving its boundary.
    assert.equal(dirname(await realpath(fixture)), temporaryRoot);
    assert.ok(fixture.startsWith(join(temporaryRoot, 'webcodex-spreadsheet-checkout-')));
    await rm(fixture, { recursive: true, force: true });
  });
  const source = join(fixture, 'source');
  await mkdir(source);
  for (const name of ['.gitattributes', vendor, ...inputs]) {
    const target = join(source, name);
    await mkdir(dirname(target), { recursive: true });
    await copyFile(join(root, name), target);
  }
  const emptyAttributes = join(fixture, 'empty-attributes');
  await writeFile(emptyAttributes, '');
  const git = (args, autocrlf = 'true') => run('git', [
    '-c', 'core.autocrlf=' + autocrlf, '-c', 'core.safecrlf=false',
    '-c', 'core.attributesFile=' + emptyAttributes, ...args,
  ], { cwd: source, timeout: 20000 });
  await git(['init', '--quiet']);
  await git(['add', '--', '.gitattributes', vendor, ...inputs]);
  const originalVendor = await readFile(join(root, vendor));
  for (const autocrlf of ['true', 'false']) {
    const checkout = join(fixture, 'checkout-' + autocrlf);
    await mkdir(checkout);
    await git(['checkout-index', '--all', '--prefix=' + checkout.replaceAll('\\', '/') + '/'], autocrlf);
    assert.equal((await readFile(join(checkout, vendor))).equals(originalVendor), true, 'Git must preserve the original vendor bytes');
    for (const name of inputs) {
      assert.equal((await readFile(join(checkout, name))).includes('\r\n'), false, name + ' must check out as LF');
    }
    const build = join(checkout, 'frontend/scripts/build-spreadsheet.mjs');
    const check = await run(process.execPath, [build, '--check'], { cwd: checkout, timeout: 20000 });
    assert.match(check.stdout, /Spreadsheet bundle checked/);
  }
  const windowsCheckout = join(fixture, 'checkout-true');
  const build = join(windowsCheckout, 'frontend/scripts/build-spreadsheet.mjs');
  // Changing only line endings still fails the exact vendor provenance check.
  await writeFile(join(windowsCheckout, vendor), originalVendor.toString('utf8').replace(/\n/g, '\r\n'));
  await assert.rejects(
    run(process.execPath, [build, '--check'], { cwd: windowsCheckout, timeout: 20000 }),
    /vendor hash changed/,
  );
});

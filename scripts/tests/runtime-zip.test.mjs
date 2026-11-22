import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { extractRuntimeZip, runtimeZipCommand } from '../extract-runtime-zip.ts';


const fixture = Buffer.from('UEsDBBQAAAAIAASOdV2DmP1AEwAAABEAAAAYAAAAcnVudGltZS9sbGFtYS1zZXJ2ZXIuZXhlK67MK8lILclMVihOLSpLLeICAFBLAwQUAAAACAAEjnVdW3k7WRQAAAASAAAAEAAAAHJ1bnRpbWUvZ2dtbC5kbGwrrswryUgtyUxWyMlMKkosquQCAFBLAQIUAxQAAAAIAASOdV2DmP1AEwAAABEAAAAYAAAAAAAAAAAAAACAAQAAAABydW50aW1lL2xsYW1hLXNlcnZlci5leGVQSwECFAMUAAAACAAEjnVdW3k7WRQAAAASAAAAEAAAAAAAAAAAAAAAgAFJAAAAcnVudGltZS9nZ21sLmRsbFBLBQYAAAAAAgACAIQAAACLAAAAAAA=', 'base64');

test('Windows ZIP extraction bypasses the archive module and stops on .NET errors', () => {
  const { command, args } = runtimeZipCommand("C:\\runner's files\\runtime.zip", "C:\\runner's files\\out", 'win32');
  assert.equal(command, 'powershell.exe');
  const script = args.at(-1);
  assert.match(script, /\$ErrorActionPreference = 'Stop'/);
  assert.match(script, /Add-Type -AssemblyName System.IO.Compression.FileSystem/);
  assert.match(script, /\[System.IO.Compression.ZipFile\]::ExtractToDirectory\('C:\\runner''s files\\runtime.zip', 'C:\\runner''s files\\out'\)/);
  assert.doesNotMatch(script, /Expand-Archive|ZipFileExtensions|Import-Module/);
});

test('runtime ZIP extracts the server and DLLs with literal spaces, quotes and shell characters in paths', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-runtime-zip-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const archive = path.join(root, "runtime's $files [test].zip");
  await writeFile(archive, fixture);
  for (const arch of ['x64', 'arm64']) {
    const target = path.join(root, `windows ${arch}'s $runtime [test]`);
    await extractRuntimeZip(archive, target);
    assert.equal(await readFile(path.join(target, 'runtime/llama-server.exe'), 'utf8'), 'synthetic server\n');
    assert.equal(await readFile(path.join(target, 'runtime/ggml.dll'), 'utf8'), 'synthetic library\n');
  }
});

test('corrupt ZIPs fail at extraction instead of succeeding with a missing server', async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-bad-runtime-zip-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const archive = path.join(root, 'invalid.zip');
  await writeFile(archive, 'not a ZIP');
  await assert.rejects(extractRuntimeZip(archive, path.join(root, 'out')), /ZIP extraction failed/);
});

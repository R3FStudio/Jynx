import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..', '..');
const require = createRequire(import.meta.url);
const commands = {
  lint: { name: 'eslint', bin: 'eslint', args: ['.', '--config', path.join(here, 'eslint.config.mjs')] },
  format: { name: 'prettier', bin: 'prettier', args: ['--write', '.'] },
  'format:check': { name: 'prettier', bin: 'prettier', args: ['--check', '.'] },
};
const command = commands[process.argv[2]];
if (!command) throw new Error('Expected lint, format, or format:check.');
const manifest = require.resolve(`${command.name}/package.json`);
const pkg = JSON.parse(await readFile(manifest, 'utf8'));
const relative = typeof pkg.bin === 'string' ? pkg.bin : pkg.bin?.[command.bin];
if (!relative) throw new Error(`${command.name} does not expose ${command.bin}.`);
const executable = path.resolve(path.dirname(manifest), relative);
const child = spawn(process.execPath, [executable, ...command.args], { cwd: root, stdio: 'inherit', shell: false });
child.once('error', (error) => { console.error(error); process.exitCode = 1; });
child.once('exit', (code) => { process.exitCode = code ?? 1; });

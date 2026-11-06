import { access, copyFile, mkdir, readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const extension = path.join(root, 'packages/private/jynx-code');
const tasks = { build: 'build', check: 'typecheck', test: 'test', ui: 'test:ui', integration: 'test:extension', packaged: 'test:packaged', package: 'package', clean: 'clean', publish: 'publish:marketplace' };
const task = process.argv[2];
if (!(task in tasks)) throw Error('Unknown extension command');
await access(path.join(extension, 'package.json')).catch(() => { throw Error('Jynx Code source is not available at packages/private/jynx-code.'); });
const npm = process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js');
const child = spawn(process.execPath, [npm, 'run', tasks[task], '--prefix', extension, '--workspaces=false', '--', ...process.argv.slice(3)], { cwd: root, stdio: 'inherit', windowsHide: true });
const code = await new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
if (code) process.exitCode = typeof code === 'number' ? code : 1;
else if (task === 'package') {
  const pkg = JSON.parse(await readFile(path.join(extension, 'package.json'), 'utf8'));
  const target = `${process.platform}-${process.arch}`;
  await mkdir(path.join(root, 'dist'), { recursive: true });
  const destination = path.join(root, `dist/Jynx-Code-v${pkg.version}-${target}.vsix`);
  await copyFile(path.join(extension, `dist/jynx-vscode-${pkg.version}-${target}.vsix`), destination);
  console.log(destination);
}

import { spawn } from 'node:child_process';
import { mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dist = path.resolve(root, '..', '..', 'dist');
const eggInfo = path.join(root, 'src', 'jynx_python_sdk.egg-info');
const interpreter = process.env.JYNX_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
async function removeGeneratedDirectory(directory) {
  const resolved = path.resolve(directory);
  const allowed = [path.join(root, 'build'), eggInfo].map(value => path.resolve(value));
  if (!allowed.includes(resolved) || path.relative(root, resolved).startsWith('..')) throw new Error('Refusing to clean outside the SDK build directories.');
  await rm(resolved, { recursive: true, force: true });
}
await mkdir(dist, { recursive: true });

await removeGeneratedDirectory(path.join(root, 'build'));
try {
  const child = spawn(interpreter, ['-m', 'pip', 'wheel', '--no-deps', '--no-cache-dir', '--wheel-dir', dist, root], { cwd: root, stdio: 'inherit', shell: false });
  const code = await new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', resolve); });
  if (code !== 0) throw new Error(`Python SDK wheel build failed (${code}).`);
} finally {
  await removeGeneratedDirectory(eggInfo);
}

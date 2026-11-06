import { access } from 'node:fs/promises';
import { createInterface } from 'node:readline/promises';
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const entry = path.join(root, 'packages/private/jynx-code/scripts/start-dev.ts');
try {
  await access(entry);
  console.log('Jynx Code will open from source in a VS Code Extension Development Host.');
  console.log('It uses a separate development profile and does not install a VSIX.');
  const input = createInterface({ input: process.stdin, output: process.stdout });
  let answer;
  try { answer = await input.question('Start Jynx Code in VS Code? [y/N] '); }
  finally { input.close(); }
  if (/^(y|yes)$/i.test(answer.trim())) {
    const child = spawn(process.execPath, [entry], { cwd: root, stdio: 'inherit', windowsHide: true });
    child.once('error', error => { console.error(error.message); process.exitCode = 1; });
    child.once('exit', code => { process.exitCode = code ?? 1; });
  } else console.log('Canceled.');
} catch (error) {
  if (error.code === 'ENOENT') console.error('The private extension source is not available at packages/private/jynx-code.');
  else console.error(error.message);
  process.exitCode = 1;
}

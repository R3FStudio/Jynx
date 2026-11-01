import path from 'node:path';
import process from 'node:process';
import { run, waitForExit } from './process-runner.ts';
import { harnessRoot, workspaceRoot } from './workspace-paths.ts';

const llama = run(process.execPath, [path.join(workspaceRoot, 'scripts', 'start-llama.ts'), ...process.argv.slice(2)], {
  cwd: workspaceRoot,
  windowsHide: true,
});
const harness = run('cargo', ['run', '--release', '--manifest-path', path.join(harnessRoot, 'Cargo.toml'), '-p', 'jynx-server', '--bin', 'jynx-harness'], {
  cwd: harnessRoot,
  windowsHide: true,
});
let stopping = false;
const stop = () => {
  if (stopping) return;
  stopping = true;
  llama.kill();
  harness.kill();
};
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
try {
  await Promise.race([waitForExit(llama, 'llama.cpp server'), waitForExit(harness, 'Jynx Harness')]);
} finally {
  stop();
}

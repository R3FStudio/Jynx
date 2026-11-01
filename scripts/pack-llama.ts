import path from 'node:path';
import process from 'node:process';
import { mkdir, readdir } from 'node:fs/promises';
import { run, waitForExit } from './process-runner.ts';
import { llamaSourceRoot, workspaceRoot } from './workspace-paths.ts';

const platform = process.platform === 'win32' ? 'windows' : process.platform === 'darwin' ? 'macos' : 'linux';
const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
const runtime = process.env.JYNX_LLAMA_RUNTIME || (platform === 'macos' ? (arch === 'arm64' ? 'metal' : 'cpu') : platform === 'windows' && arch === 'arm64' ? 'cpu' : 'vulkan');
const binaries = path.join(llamaSourceRoot, 'binaries');
const relative = path.join(platform, arch, runtime);
const serverName = platform === 'windows' ? 'llama-server.exe' : 'llama-server';
async function hasServer(directory: string): Promise<boolean> {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
    const candidate = path.join(directory, entry.name);
    if (entry.isFile() && entry.name === serverName) return true;
    if (entry.isDirectory() && await hasServer(candidate)) return true;
  }
  return false;
}
if (!(await hasServer(path.join(binaries, relative)))) {
  const sync = run(process.execPath, [path.join(workspaceRoot, 'scripts', 'sync-llama.ts'), '--platform', platform, '--arch', arch, '--runtime', runtime], { cwd: workspaceRoot });
  await waitForExit(sync, 'llama.cpp runtime sync');
}
const dist = path.join(workspaceRoot, 'dist');
await mkdir(dist, { recursive: true });
const archive = path.join(dist, `Jynx-LlamaCpp-Runtime-${platform}-${arch}-${runtime}.tar.gz`);
const child = run('tar', ['-czf', archive, '-C', binaries, relative], { cwd: llamaSourceRoot });
await waitForExit(child, 'llama.cpp archive');
console.log(archive);

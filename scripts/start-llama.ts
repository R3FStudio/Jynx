import path from 'node:path';
import process from 'node:process';
import { access, readFile, readdir } from 'node:fs/promises';
import { run, waitForExit } from './process-runner.ts';
import { llamaSourceRoot } from './workspace-paths.ts';

const platform = process.platform === 'win32' ? 'windows' : process.platform === 'darwin' ? 'macos' : 'linux';
const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
const runtime = process.env.JYNX_LLAMA_RUNTIME || (platform === 'macos' ? (arch === 'arm64' ? 'metal' : 'cpu') : platform === 'windows' && arch === 'arm64' ? 'cpu' : 'vulkan');
const binaryRoot = path.join(llamaSourceRoot, 'binaries');
const runtimeRoot = path.join(binaryRoot, platform, arch, runtime);
const serverName = process.platform === 'win32' ? 'llama-server.exe' : 'llama-server';
async function findServer(directory: string): Promise<string | null> {
  for (const entry of await readdir(directory, { withFileTypes: true }).catch(() => [])) {
    const candidate = path.join(directory, entry.name);
    if (entry.isFile() && entry.name === serverName) return candidate;
    if (entry.isDirectory()) {
      const nested = await findServer(candidate);
      if (nested) return nested;
    }
  }
  return null;
}
const args = process.argv.slice(2);
const modelIndex = args.indexOf('--model');
const model = modelIndex >= 0 ? args[modelIndex + 1] : process.env.JYNX_LLAMA_MODEL;
if (!model || model.startsWith('--')) throw new Error('Pass --model <model.gguf> or set JYNX_LLAMA_MODEL.');
const modelPath = path.resolve(model);
let server: string | null = null;
try {
  const manifest = JSON.parse(await readFile(path.join(binaryRoot, 'manifest.json'), 'utf8'));
  const relative = manifest?.runtimes?.[`${platform}/${arch}/${runtime}`]?.server;
  if (typeof relative === 'string' && relative && !path.isAbsolute(relative)) {
    const candidate = path.resolve(runtimeRoot, relative);
    const inside = path.relative(runtimeRoot, candidate);
    if (inside !== '..' && !inside.startsWith(`..${path.sep}`) && !path.isAbsolute(inside)) {
      await access(candidate);
      server = candidate;
    }
  }
} catch {}
server ||= await findServer(runtimeRoot);
if (!server) throw new Error(`Missing llama.cpp server in ${runtimeRoot}. Run npm run llama:sync -- --platform ${platform} --arch ${arch} --runtime ${runtime} first.`);
await access(modelPath);
const forwarded = modelIndex >= 0 ? args.filter((_value, index) => index !== modelIndex && index !== modelIndex + 1) : args;
const child = run(server, ['--model', modelPath, ...forwarded], { cwd: path.dirname(server), windowsHide: true });
process.on('SIGINT', () => child.kill());
process.on('SIGTERM', () => child.kill());
await waitForExit(child, 'llama.cpp server');

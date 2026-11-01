import path from 'node:path';
import process from 'node:process';
import { mkdir, mkdtemp, rm, readFile, rename } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { run, waitForExit } from './process-runner.ts';
import { workspaceRoot } from './workspace-paths.ts';

if (!process.env.npm_execpath) throw new Error('Run this script with npm.');
const cache = await mkdtemp(path.join(tmpdir(), 'jynx-sdk-pack-'));
try {
  const destination = path.join(workspaceRoot, 'dist');
  await mkdir(destination, { recursive: true });
  const child = run(process.execPath, [process.env.npm_execpath, 'pack', '--workspace', '@jynx/sdk', '--pack-destination', destination, '--cache', cache], { cwd: workspaceRoot });
  await waitForExit(child, 'JavaScript SDK pack');
  const { version } = JSON.parse(await readFile(path.join(workspaceRoot, 'packages/sdk-js/package.json'), 'utf8'));
  const archive = path.join(destination, `Jynx-JavaScript-SDK-v${version}.tgz`);
  await rename(path.join(destination, `jynx-sdk-${version}.tgz`), archive);
  console.log(archive);
} finally {
  const relative = path.relative(path.resolve(tmpdir()), path.resolve(cache));
  if (!relative || relative.startsWith('..') || path.isAbsolute(relative) || !path.basename(cache).startsWith('jynx-sdk-pack-')) throw new Error('Refusing to clean outside the generated SDK pack cache.');
  await rm(path.resolve(cache), { recursive: true, force: true });
}

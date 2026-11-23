import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { inspectComponents } from './workspace-components.mjs';

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function npmCommand(npmCli, directory, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [npmCli, ...args, '--prefix', directory,
      ...(directory !== workspaceRoot ? ['--workspaces=false'] : [])], {
      cwd: directory, stdio: 'inherit', windowsHide: true, env,
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => code === 0 ? resolve()
      : reject(new Error(`Dependency installation failed in ${directory} (${signal || code})`)));
  });
}

async function runInstall(directory, { command = 'ci' }) {
  const nodeDirectory = path.dirname(process.execPath);
  const npmCli = process.env.npm_execpath || [
    path.join(nodeDirectory, 'node_modules/npm/bin/npm-cli.js'),
    path.join(nodeDirectory, '../lib/node_modules/npm/bin/npm-cli.js'),
  ].find(existsSync);
  if (!npmCli) throw new Error('Cannot locate npm. Run this installer with npm run install:all.');
  const env = { ...process.env, JYNX_SKIP_STANDALONE_INSTALL: '1' };
  const linux = process.platform === 'linux';
  await npmCommand(npmCli, directory, [command, '--include=dev', '--include=optional',
    ...(linux ? ['--ignore-scripts'] : [])], env);
  if (!linux) return;
  const require = createRequire(path.join(directory, 'package.json'));
  let esbuild;
  try { esbuild = require.resolve(`@esbuild/linux-${process.arch}/bin/esbuild`); }
  catch (error) { if (error.code !== 'MODULE_NOT_FOUND') throw error; }
  if (esbuild) {
    const { executablePath } = await import('./process-runner.ts');
    env.ESBUILD_BINARY_PATH = await executablePath(esbuild);
  }
  await npmCommand(npmCli, directory, ['rebuild', '--foreground-scripts'], env);
}

export async function installDependencies({ root = workspaceRoot, install = runInstall, log = console.log,
  standaloneOnly = false, command = standaloneOnly ? 'install' : 'ci' } = {}) {
  if (!['install', 'ci'].includes(command)) throw new Error('Unsupported dependency installation command.');

  const { components } = await inspectComponents(root);
  const invalid = components.filter((component) => component.status === 'invalid');
  if (invalid.length) throw new Error(invalid.map((component) => `${component.id}: ${component.reason}`).join('\n'));
  const directories = [
    ...(standaloneOnly ? [] : [root]),
    ...components
      .filter((component) => component.standalone && component.manifest)
      .map((component) => component.directory),
  ];
  for (const directory of directories) {
    log(`[install:all] Installing dependencies in ${path.relative(root, directory) || '.'}`);
    await install(directory, { standalone: directory !== root, command });
  }
  return directories;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  if (process.env.JYNX_SKIP_STANDALONE_INSTALL !== '1') {
    const standaloneOnly = process.argv.includes('--standalone-only');
    await installDependencies({ standaloneOnly,
      command: standaloneOnly && process.env.npm_command !== 'ci' ? 'install' : 'ci' });
  }
}

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { inspectComponents } from './workspace-components.mjs';

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function runInstall(directory, { standalone }) {
  const nodeDirectory = path.dirname(process.execPath);
  const npmCli =
    process.env.npm_execpath ||
    [
      path.join(nodeDirectory, 'node_modules/npm/bin/npm-cli.js'),
      path.join(nodeDirectory, '../lib/node_modules/npm/bin/npm-cli.js'),
    ].find(existsSync);
  if (!npmCli) throw new Error('Cannot locate npm. Run this installer with npm run install:all.');
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [
        npmCli,
        'ci',
        '--include=dev',
        '--include=optional',
        '--prefix',
        directory,
        ...(standalone ? ['--workspaces=false'] : []),
      ],
      {
        cwd: directory,
        stdio: 'inherit',
        windowsHide: true,
      },
    );
    child.once('error', reject);
    child.once('exit', (code, signal) =>
      code === 0 ? resolve() : reject(new Error(`Dependency installation failed in ${directory} (${signal || code})`)),
    );
  });
}

export async function installDependencies({ root = workspaceRoot, install = runInstall, log = console.log } = {}) {
  // Inspect before installing so malformed component metadata cannot trigger partial installation.
  const { components } = await inspectComponents(root);
  const invalid = components.filter((component) => component.status === 'invalid');
  if (invalid.length) throw new Error(invalid.map((component) => `${component.id}: ${component.reason}`).join('\n'));
  const directories = [
    root,
    ...components
      .filter((component) => component.standalone && component.manifest)
      .map((component) => component.directory),
  ];
  for (const directory of directories) {
    log(`[install:all] Installing dependencies in ${path.relative(root, directory) || '.'}`);
    await install(directory, { standalone: directory !== root });
  }
  return directories;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await installDependencies();
}

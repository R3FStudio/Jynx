import { run, waitForExit } from './process-runner.ts';
import { workspaceRoot } from './workspace-paths.ts';
import { inspectComponents } from './workspace-components.mjs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export async function buildAvailableComponents({ root = workspaceRoot, exclude = [] as string[], log = console.log, reportError = console.error } = {}) {
  const { components } = await inspectComponents(root, {exclude});
  const built: string[] = [];
  const skipped: string[] = [];
  const failed: string[] = [];
  const npmCli = process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');
  for (const component of components) {
    if (component.status === 'invalid') {
      failed.push(component.id);
      reportError(`[build:all] ${component.id}: ${component.reason}`);
      continue;
    }
    if (component.status !== 'ready') {
      skipped.push(component.id);
      log(`[build:all] Skipping ${component.id}: ${component.reason}`);
      continue;
    }
    log(`[build:all] Building ${component.id} (${component.name})`);
    try {


      const child = run(process.execPath, [npmCli, 'run', 'build', '--prefix', component.directory, '--workspaces=false'], { cwd: root });
      await waitForExit(child, `${component.id} build`);
      built.push(component.id);
    } catch (error) {
      failed.push(component.id);
      reportError(`[build:all] ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  log(`[build:all] Finished: ${built.length} built, ${skipped.length} skipped, ${failed.length} failed.`);
  return { built, skipped, failed };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const result = await buildAvailableComponents({exclude: process.argv.includes('--exclude-desktop') ? ['desktop'] : []});
  if (result.failed.length) process.exitCode = 1;
}

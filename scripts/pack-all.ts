import process from 'node:process';
import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { run, waitForExit } from './process-runner.ts';
import { workspaceRoot } from './workspace-paths.ts';

type PackageStep = { id: string; args: string[] };
export async function packagingPlan({ root = workspaceRoot, args = [] as string[] } = {}) {
  const known = new Set(['--include-desktop', '--exclude-desktop', '--installers-only', '--bundle-only', '--desktop-only', '--dir', '--dry-run']);
  const desktopArgs: string[] = [];
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === '--release' || arg === '--check') desktopArgs.push(arg);
    else if (arg === '--platform' || arg === '--arch') {
      const value = args[++index];
      if (!value || value.startsWith('--')) throw new Error(`Missing value for ${arg}`);
      desktopArgs.push(arg, value);
    } else if (!known.has(arg)) throw new Error(`Unknown packaging option: ${arg}`);
  }
  if (desktopArgs.length && (!args.includes('--desktop-only') || args.includes('--exclude-desktop'))) throw new Error('Desktop target/release options require --desktop-only.');
  if (args.includes('--include-desktop') && args.includes('--exclude-desktop')) throw new Error('Choose --include-desktop or --exclude-desktop, not both.');
  const modes = ['--installers-only', '--bundle-only', '--desktop-only'].filter(flag => args.includes(flag));
  if (modes.length > 1) throw new Error('Choose one packaging mode.');
  const mode = modes[0] || 'all';
  const archiveOnly = mode === '--bundle-only' || args.includes('--dir');
  const steps: PackageStep[] = [];
  if (mode === 'all') {
    for (const id of ['harness', 'cli', 'llama', 'sdk-js', 'sdk-python']) steps.push({ id, args: ['run', `pack:${id}`] });
  }
  if (mode !== '--desktop-only') steps.push({ id: archiveOnly ? 'bundle' : 'installers', args: [path.join(root, 'scripts', 'pack-installers.mjs'), ...(archiveOnly ? ['--archive-only'] : [])] });
  const includeDesktop = !args.includes('--exclude-desktop') && (mode === '--desktop-only' || args.includes('--include-desktop'));
  if (includeDesktop) {
    const config = JSON.parse(await readFile(path.join(root, 'workspace.config.json'), 'utf8'));
    const relative = config.packages?.desktop?.path;
    if (typeof relative !== 'string') throw new Error('Desktop component is not configured.');
    const directory = path.resolve(root, relative);
    const inside = path.relative(root, directory);
    if (!inside || inside.startsWith('..') || path.isAbsolute(inside)) throw new Error('Desktop checkout must be inside the workspace.');
    try { await access(path.join(directory, 'package.json')); }
    catch { throw new Error('Desktop checkout is not installed. Run npm run desktop:clone, or choose --exclude-desktop.'); }
    steps.push({ id: 'desktop', args: ['run', archiveOnly ? 'pack' : 'package', '--prefix', directory, '--workspaces=false', ...(desktopArgs.length ? ['--', ...desktopArgs] : [])] });
  }
  return { steps, excludedDesktop: args.includes('--exclude-desktop'), dryRun: args.includes('--dry-run') };
}

export async function packageComponents({ root = workspaceRoot, args = [] as string[], log = console.log } = {}) {
  const plan = await packagingPlan({ root, args });
  let npmCli: string | undefined;
  for (const candidate of [process.env.npm_execpath,
    path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js'),
    path.join(path.dirname(process.execPath), '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js')]) {
    if (!candidate) continue;
    try { await access(candidate); npmCli = candidate; break; } catch {}
  }
  if (!npmCli) throw new Error('npm CLI could not be found. Run packaging through npm or install npm beside Node.');
  if (plan.excludedDesktop) log('[pack] Desktop excluded.');
  for (const step of plan.steps) {
    log(`[pack] ${plan.dryRun ? 'Would package' : 'Packaging'} ${step.id}`);
    if (plan.dryRun) continue;
    const commandArgs = step.id === 'installers' || step.id === 'bundle' ? step.args : [npmCli, ...step.args];
    await waitForExit(run(process.execPath, commandArgs, { cwd: root }), `${step.id} packaging`);
  }
  return plan;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try { await packageComponents({ args: process.argv.slice(2) }); }
  catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}

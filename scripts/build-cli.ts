import path from 'node:path';
import { run, waitForExit } from './process-runner.ts';
import { cliRoot } from './workspace-paths.ts';

const cli = run('cargo', ['build', '--release', '--manifest-path', path.join(cliRoot, 'Cargo.toml'), '--bin', 'cli'], { cwd: cliRoot });
await waitForExit(cli, 'CLI build');

import { run, waitForExit } from './process-runner.ts';
import { workspaceRoot } from './workspace-paths.ts';

const js = run(process.execPath, ['--test', 'packages/sdk-js/test/client.test.js', 'packages/sdk-js/test/discovery.test.js', 'packages/sdk-js/test/harness.test.js', 'packages/sdk-js/test/stream.test.js'], { cwd: workspaceRoot });
await waitForExit(js, 'JavaScript SDK checks');
const python = run(process.platform === 'win32' ? 'python.exe' : 'python3', ['-m', 'unittest', 'discover', '-s', 'packages/sdk-python/tests'], { cwd: workspaceRoot });
await waitForExit(python, 'Python SDK checks');

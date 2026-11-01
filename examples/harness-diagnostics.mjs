import { join } from 'node:path';
import { HarnessClient } from '../packages/sdk-js/src/harness/client.js';

const binaryPath =
  process.argv[2] ||
  join(
    process.cwd(),
    'packages',
    'harness',
    'target',
    'release',
    process.platform === 'win32' ? 'jynx-harness.exe' : 'jynx-harness',
  );
const harness = new HarnessClient({ binaryPath });
try {
  console.log(await harness.diagnostics());
} finally {
  harness.close();
}

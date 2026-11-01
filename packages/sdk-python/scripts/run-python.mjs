import { spawn } from 'node:child_process';
import process from 'node:process';

const interpreter = process.env.JYNX_PYTHON || (process.platform === 'win32' ? 'python' : 'python3');
const child = spawn(interpreter, process.argv.slice(2), { stdio: 'inherit', shell: false });
child.on('error', (error) => { console.error(error); process.exitCode = 1; });
child.on('exit', (code) => { process.exitCode = code ?? 1; });

import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';

export function runtimeZipCommand(archive: string, destination: string, platform: NodeJS.Platform = process.platform) {
  if (platform !== 'win32') return { command: 'unzip', args: ['-o', archive, '-d', destination] };
  const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
  // Windows PowerShell can inherit a PowerShell 7 PSModulePath from the runner.
  // Bypass Microsoft.PowerShell.Archive and its incompatible overwrite overload.
  // The caller stages each archive into a fresh directory, so no overwrite API
  // is needed. This two-argument overload works with .NET Framework as well.
  const script = "$ErrorActionPreference = 'Stop'; "
    + 'Add-Type -AssemblyName System.IO.Compression.FileSystem; '
    + `[System.IO.Compression.ZipFile]::ExtractToDirectory(${literal(archive)}, ${literal(destination)})`;
  return { command: 'powershell.exe', args: ['-NoProfile', '-NonInteractive', '-Command', script] };
}

export async function extractRuntimeZip(archive: string, destination: string) {
  await mkdir(destination, { recursive: true });
  const { command, args } = runtimeZipCommand(archive, destination);
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, { stdio: 'inherit', shell: false, windowsHide: true });
    child.once('error', reject);
    child.once('exit', (code, signal) => code === 0 ? resolve() : reject(new Error(`ZIP extraction failed: ${command} exited with ${code ?? signal}`)));
  });
}

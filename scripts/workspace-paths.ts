import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const packagesRoot = path.join(workspaceRoot, 'packages');
export const desktopRoot = path.join(packagesRoot, 'private', 'Desktop');
export const harnessRoot = path.join(packagesRoot, 'harness');
export const cliRoot = path.join(packagesRoot, 'cli');
export const rustTargetRoot = path.join(harnessRoot, 'target');
export const llamaSourceRoot = path.join(workspaceRoot, 'vendor', 'llama.cpp');

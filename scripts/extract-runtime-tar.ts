import path from 'node:path';
import { copyFile, mkdir, stat, realpath, rm, symlink } from 'node:fs/promises';
import { x } from 'tar';

export async function extractRuntimeTar(archive: string, destination: string, materializeLinks = process.platform === 'win32') {
  const root = path.resolve(destination);
  const inside = (name: string) => {
    if (path.isAbsolute(name) || path.win32.isAbsolute(name)) throw new Error(`Unsafe archive path: ${name}`);
    const resolved = path.resolve(root, name);
    const relative = path.relative(root, resolved);
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Unsafe archive path: ${name}`);
    return resolved;
  };
  const links = new Map<string, string>();
  const nativeLinks = new Set<string>();
  await mkdir(root, { recursive: true });
  const canonicalRoot = await realpath(root);
  const checkCanonical = (target: string) => {
    const relative = path.relative(canonicalRoot, target);
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Archive link escapes destination: ${target}`);
  };
  await x({
    file: archive,
    cwd: root,
    strict: true,
    preserveOwner: false,
    filter: (name, entry) => {
      inside(name);
      if (entry.type === 'SymbolicLink' || entry.type === 'Link') {
        if (path.isAbsolute(entry.linkpath) || path.win32.isAbsolute(entry.linkpath)) throw new Error(`Unsafe archive link: ${entry.linkpath}`);
        const target = entry.type === 'SymbolicLink' ? path.join(path.dirname(name), entry.linkpath) : entry.linkpath;
        const resolvedTarget = inside(target);
        const alias = inside(name);
        links.set(alias, resolvedTarget);
        if (!materializeLinks && entry.type === 'SymbolicLink') nativeLinks.add(alias);
        return false;
      }
      return true;
    },
  });


  const sourceFor = (target: string, seen = new Set<string>()): string => {
    if (seen.has(target)) throw new Error(`Cyclic archive link: ${target}`);
    seen.add(target);
    const next = links.get(target);
    return next ? sourceFor(next, seen) : target;
  };
  for (const [name, target] of links) {
    const source = sourceFor(target);
    checkCanonical(await realpath(source));
    if (!(await stat(source)).isFile()) throw new Error(`Runtime archive link is not a file: ${name}`);
    await mkdir(path.dirname(name), { recursive: true });
    checkCanonical(await realpath(path.dirname(name)));
    await rm(name, { force: true });
    if (nativeLinks.has(name)) await symlink(path.relative(path.dirname(name), target), name);
    else await copyFile(source, name);
  }
}

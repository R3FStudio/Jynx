import path from 'node:path';
import {readFile, writeFile, mkdir, access} from 'node:fs/promises';
import {fileURLToPath, pathToFileURL} from 'node:url';
const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export function privacyText(markdown) {
  return markdown.replace(/^#{1,6} /gm, '').replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '$1 ($2)').replace(/`([^`]+)`/g, '$1');
}
export function privacyHtml(markdown) {
  const escape = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
  const inline = value => escape(value).replace(/`([^`]+)`/g, '<code>$1</code>').replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>');
  const body = markdown.trim().split(/\n\s*\n/).map(block => {
    const heading = /^(#{1,3}) (.+)$/.exec(block);
    if (heading) return `<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`;
    if (block.startsWith('- ')) return '<ul>' + block.split('\n').map(line => `<li>${inline(line.replace(/^- /, ''))}</li>`).join('') + '</ul>';
    return `<p>${inline(block)}</p>`;
  }).join('\n');
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Jynx Privacy Policy</title><style>body{font:14px -apple-system,BlinkMacSystemFont,Arial,sans-serif;line-height:1.65;margin:28px;color:#222;background:#fff;overflow-wrap:anywhere}h1{font-size:26px;line-height:1.2}h2{font-size:18px;margin-top:26px}li{margin:6px 0}code{font-size:13px;background:#f2f2f2;padding:1px 3px}a{color:#295aa8}</style></head><body>${body}</body></html>`;
}
export async function syncPrivacy(root = workspace) {
  const text = await readFile(path.join(root, 'PRIVACY.md'), 'utf8');
  const targets = ['packages/sdk-js/PRIVACY.md', 'packages/sdk-python/PRIVACY.md', 'packages/sdk-python/src/jynx_sdk/PRIVACY.md'];
  for (const target of targets) {
    await mkdir(path.dirname(path.join(root, target)), {recursive: true});
    await writeFile(path.join(root, target), text);
  }
  const desktop = path.join(root, 'packages/private/Desktop');
  if (await access(path.join(desktop, 'package.json')).then(() => true, () => false)) {
    await mkdir(path.join(desktop, 'assets/privacy'), {recursive: true});
    await writeFile(path.join(desktop, 'PRIVACY.md'), text);
    await writeFile(path.join(desktop, 'assets/privacy/PRIVACY.txt'), privacyText(text));
    await writeFile(path.join(desktop, 'assets/privacy/PRIVACY.html'), privacyHtml(text));
  }
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) await syncPrivacy();

import assert from 'node:assert/strict';
import { mkdtemp, readFile, readlink, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { gzipSync } from 'node:zlib';
import { Header } from 'tar';
import { extractRuntimeTar } from '../extract-runtime-tar.ts';

function archive(entries) {
  const chunks = [];
  for (const { contents = '', ...entry } of entries) {
    const data = Buffer.from(contents);
    const header = new Header({ mode: 0o755, size: data.length, ...entry });
    header.encode();
    chunks.push(header.block);
    if (data.length) chunks.push(data, Buffer.alloc((512 - data.length % 512) % 512));
  }
  return gzipSync(Buffer.concat([...chunks, Buffer.alloc(1024)]));
}

test('Unix shared-library aliases and link chains extract on Windows without symlink privileges', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'jynx-runtime-tar-'));
  try {
    const file = path.join(root, 'runtime.tar.gz');
    await writeFile(file, archive([
      { path: 'bin/lib.so', type: 'SymbolicLink', linkpath: 'lib.so.0' },
      { path: 'bin/lib.so.0', type: 'SymbolicLink', linkpath: 'lib.so.0.1' },
      { path: 'bin/lib-copy.so', type: 'Link', linkpath: 'bin/lib.so.0.1' },
      { path: 'bin/lib.so.0.1', type: 'File', contents: 'shared library' },
      { path: 'bin/llama-server', type: 'File', contents: 'native server' },
    ]));
    const target = path.join(root, 'extracted');
    await extractRuntimeTar(file, target, true);
    for (const name of ['lib.so', 'lib.so.0', 'lib-copy.so', 'lib.so.0.1']) {
      assert.equal(await readFile(path.join(target, 'bin', name), 'utf8'), 'shared library');
    }
    assert.equal(await readFile(path.join(target, 'bin/llama-server'), 'utf8'), 'native server');
    if (process.platform !== 'win32') {
      const nativeTarget = path.join(root, 'native');
      await extractRuntimeTar(file, nativeTarget, false);
      assert.equal(await readlink(path.join(nativeTarget, 'bin/lib.so')), 'lib.so.0');
      assert.equal(await readFile(path.join(nativeTarget, 'bin/lib.so'), 'utf8'), 'shared library');
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('real library files followed by reverse-order aliases extract without traversing symlinks',async t=>{
 const root=await mkdtemp(path.join(os.tmpdir(),'jynx-tar-alias-order-'));t.after(()=>rm(root,{recursive:true,force:true}));
 const file=path.join(root,'runtime.tar.gz');await writeFile(file,archive([
 {path:'bin/lib.so.1',type:'File',contents:'library'},
 {path:'bin/lib.so.0',type:'SymbolicLink',linkpath:'lib.so.1'},
 {path:'bin/lib.so',type:'SymbolicLink',linkpath:'lib.so.0'},
 ]));
 for(const materialize of process.platform==='win32'?[true]:[true,false]){
  const target=path.join(root,String(materialize));await extractRuntimeTar(file,target,materialize);
  assert.equal(await readFile(path.join(target,'bin/lib.so'),'utf8'),'library');
 }
});

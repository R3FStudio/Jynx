import assert from 'node:assert/strict';import test from 'node:test';
import {mkdtemp,mkdir,writeFile,readFile,rm,chmod,copyFile,access} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';import {list as listTar} from 'tar';
import os from 'node:os';import path from 'node:path';
import {stageBundle,packageBundle,inside} from '../pack-installers.mjs';
async function fixture(t){
 const root=await mkdtemp(path.join(os.tmpdir(),'jynx-installers-'));t.after(()=>rm(root,{recursive:true,force:true}));
 const files={
  'package.json':JSON.stringify({version:'0.0.1'}),'.gitignore':'.cache/\ndist/\nvendor/\n**/target/\npackages/private/\n',
  'PRIVACY.md':'# Fixture Privacy Policy\n\n## Local storage\n\nSynthetic privacy text.\n','LICENSE':'Synthetic GPL notice','COPYRIGHT':'R3F Studio','README.md':'Synthetic source',
  'assets/branding/LICENSE.txt':'Synthetic artwork notice','assets/branding/jynx.ico':'Synthetic icon',
  'packages/private/Desktop/private.txt':'PRIVATE_CANARY',
  'packages/cli/target/release/jynx-cli':'#!/bin/sh\necho "Jynx CLI 0.0.1"\n',
  'packages/harness/target/release/jynx-harness':'#!/bin/sh\necho "Jynx Harness 0.0.1"\n',
  'vendor/llama.cpp/source/LICENSE':'Synthetic MIT notice',
  'vendor/llama.cpp/binaries/manifest.json':JSON.stringify({runtimes:Object.fromEntries(['cpu','vulkan'].map(id=>['linux/x64/'+id,{server:'release/llama-server',release:'fixture',commit:'fixture'}]))}),
 };
 for(const backend of ['cpu','vulkan']){files[`vendor/llama.cpp/binaries/linux/x64/${backend}/release/llama-server`]='#!/bin/sh\necho "llama fixture"\n';files[`vendor/llama.cpp/binaries/linux/x64/${backend}/release/libdependency.so`]='LIBRARY_CANARY';}
 for(const [relative,text] of Object.entries(files)){const file=path.join(root,relative);await mkdir(path.dirname(file),{recursive:true});await writeFile(file,text);if(text.startsWith('#!'))await chmod(file,0o755);}
 execFileSync('git',['init'],{cwd:root,stdio:'ignore'});return root;
}
test('bundle contains native tools, complete runtime folders, notices, and public source without private files',async t=>{
 const root=await fixture(t);const bundle=await stageBundle({root,platform:'linux',arch:'x64'});
 const manifest=JSON.parse(await readFile(path.join(bundle.payload,'runtime-manifest.json')));assert.equal(manifest.defaultRuntime,'cpu');assert.deepEqual(manifest.runtimes.map(x=>x.id),['cpu','vulkan']);
 assert.equal(await readFile(path.join(bundle.payload,'runtimes/vulkan/release/libdependency.so'),'utf8'),'LIBRARY_CANARY');
 assert.match(await readFile(path.join(bundle.payload,'PRIVACY.txt'),'utf8'),/Fixture Privacy Policy/);assert.match(await readFile(path.join(bundle.staging,'mac-resources/Privacy.html'),'utf8'),/<h1>Fixture Privacy Policy<\/h1>/);
 await access(path.join(bundle.payload,'LICENSE'));await access(path.join(bundle.payload,'licenses/llama.cpp/LICENSE'));
 const source=[];await listTar({file:path.join(bundle.payload,'source/jynx-source.tar.gz'),onReadEntry:entry=>source.push(entry.path)});
 assert.ok(source.includes('README.md'));assert.ok(!source.some(file=>file.startsWith('packages/private/')));assert.ok(!source.some(file=>file.includes('/target/')));
 const result=await packageBundle(bundle);assert.equal(path.basename(result.archive),'Jynx-Complete-Portable-v0.0.1-linux-x64.tar.gz');assert.equal(path.basename(result.installer),'Jynx-Complete-Setup-v0.0.1-linux-x64.run');assert.equal(path.dirname(result.installer),path.join(root,'dist'));assert.equal(path.dirname(result.archive),path.join(root,'dist'));const data=await readFile(result.installer);assert.ok(data.includes(Buffer.from('__JYNX_PAYLOAD__\n')));await access(result.installer+'.sha256');
});
test('bundle refuses runtime paths outside the selected runtime tree',async t=>{
 const root=await fixture(t);const file=path.join(root,'vendor/llama.cpp/binaries/manifest.json');const manifest=JSON.parse(await readFile(file));manifest.runtimes['linux/x64/cpu'].server='../../outside';await writeFile(file,JSON.stringify(manifest));
 await assert.rejects(stageBundle({root,platform:'linux',arch:'x64'}),/Unsafe bundle path/);assert.throws(()=>inside(root,'C:\\outside'),/Unsafe/);
});
test('Unix setup installs commands, upgrades without losing local files, and uninstalls only owned files',{skip:process.platform==='win32'},async t=>{
 const root=await fixture(t);const bundle=await stageBundle({root,platform:'linux',arch:'x64'});
 const prefix=path.join(root,'installed app');const bin=path.join(root,'commands');const args=[path.join(bundle.payload,'install.sh'),'--prefix',prefix,'--bin-dir',bin];
 const notice=execFileSync('sh',[path.join(bundle.payload,'install.sh'),'--privacy','--prefix',prefix,'--bin-dir',bin],{encoding:'utf8'});assert.match(notice,/Synthetic privacy text/);await assert.rejects(access(prefix));
 const installedOutput=execFileSync('sh',args,{encoding:'utf8',timeout:10000});assert.match(installedOutput,/privacy policy/i);assert.equal(await readFile(path.join(prefix,'PRIVACY.md'),'utf8'),await readFile(path.join(root,'PRIVACY.md'),'utf8'));assert.match(execFileSync(path.join(bin,'jynx-cli'),['--version'],{encoding:'utf8'}),/0.0.1/);assert.match(execFileSync(path.join(bin,'jynx-llama'),['--version'],{encoding:'utf8'}),/llama fixture/);
 await writeFile(path.join(prefix,'keep.txt'),'LOCAL_DATA');execFileSync('sh',args);assert.equal(await readFile(path.join(prefix,'keep.txt'),'utf8'),'LOCAL_DATA');
 execFileSync('sh',[path.join(prefix,'uninstall.sh')]);assert.equal(await readFile(path.join(prefix,'keep.txt'),'utf8'),'LOCAL_DATA');await assert.rejects(access(path.join(prefix,'PRIVACY.md')));await assert.rejects(access(path.join(bin,'jynx-cli')));
});

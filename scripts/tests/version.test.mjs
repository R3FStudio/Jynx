import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp,mkdir,writeFile,readFile,rm} from 'node:fs/promises';
import os from 'node:os';import path from 'node:path';
import {synchronizeVersion} from '../version.mjs';
test('version command detects drift, updates owned manifests and locks, and preserves dependency versions',async t=>{
 const directory=await mkdtemp(path.join(os.tmpdir(),'jynx-version-'));t.after(()=>rm(directory,{recursive:true,force:true}));
 const files={
 'package.json':JSON.stringify({version:'0.0.1'}),
 'workspace.config.json':JSON.stringify({packages:{cli:{path:'packages/cli'},desktop:{path:'packages/private/Desktop'}}}),
 'packages/cli/package.json':JSON.stringify({version:'0.1.0'}),
 'packages/cli/Cargo.toml':'[package]\nname = "jynx-cli"\nversion = "0.1.0"\n[dependencies]\nsemver = "1"\n',
 'packages/cli/Cargo.lock':'version = 4\n\n[[package]]\nname = "jynx-cli"\nversion = "0.1.0"\n\n[[package]]\nname = "serde"\nversion = "1.0.0"\nsource = "registry+example"\n',
 'package-lock.json':JSON.stringify({version:'0.0.1',packages:{'':{version:'0.0.1'},'packages/cli':{version:'0.1.0'},'node_modules/foreign':{version:'4.2.0'}}})
 };
 for(const [file,text] of Object.entries(files)){await mkdir(path.dirname(path.join(directory,file)),{recursive:true});await writeFile(path.join(directory,file),text);}
 assert.ok((await synchronizeVersion({directory})).mismatches.includes('packages/cli/package.json'));
 await synchronizeVersion({directory,version:'0.0.2',write:true});assert.deepEqual((await synchronizeVersion({directory})).mismatches,[]);
 const lock=JSON.parse(await readFile(path.join(directory,'package-lock.json')));assert.equal(lock.packages['node_modules/foreign'].version,'4.2.0');
 const cargo=await readFile(path.join(directory,'packages/cli/Cargo.lock'),'utf8');assert.match(cargo,/name = "jynx-cli"\nversion = "0.0.2"/);assert.match(cargo,/name = "serde"\nversion = "1.0.0"/);
 await assert.rejects(synchronizeVersion({directory,version:'latest',write:true}),/semantic version/);
});

import assert from 'node:assert/strict';import test from 'node:test';
import {mkdtemp,mkdir,writeFile,readFile,rm,access} from 'node:fs/promises';import {spawnSync} from 'node:child_process';
import path from 'node:path';import os from 'node:os';import {pathToFileURL} from 'node:url';
import {packagingPlan} from '../pack-all.ts';
const entry=pathToFileURL(path.resolve(import.meta.dirname,'../pack-all.ts')).href;
async function fixture(t,{desktop=true,malformed=false}={}){
 const root=await mkdtemp(path.join(os.tmpdir(),'jynx-package-selection-'));t.after(()=>rm(root,{recursive:true,force:true}));await mkdir(path.join(root,'scripts'));
 const scripts=Object.fromEntries(['harness','cli','llama','sdk-js','sdk-python'].map(name=>['pack:'+name,`node scripts/mark.mjs ${name}`]));
 await writeFile(path.join(root,'package.json'),JSON.stringify({private:true,scripts}));
 await writeFile(path.join(root,'workspace.config.json'),JSON.stringify({packages:{desktop:{path:'packages/private/Desktop'}}}));
 await writeFile(path.join(root,'scripts/mark.mjs'),`import {appendFileSync} from 'node:fs';import path from 'node:path';appendFileSync(path.resolve(process.cwd(),process.env.PACK_REPORT),' '+process.argv[2]);`);
 await writeFile(path.join(root,'scripts/pack-installers.mjs'),`import {appendFileSync} from 'node:fs';appendFileSync(process.env.PACK_REPORT,process.argv.includes('--archive-only')?' bundle':' installers');`);
 if(desktop){const dir=path.join(root,'packages/private/Desktop');await mkdir(dir,{recursive:true});await writeFile(path.join(dir,'package.json'),malformed?'not JSON':JSON.stringify({scripts:{pack:'node ../../../scripts/mark.mjs desktop-dir',package:'node ../../../scripts/mark.mjs desktop-installer'}}));}
 return root;
}
function execute(root,args){const report=path.join(root,'result.txt');return spawnSync(process.execPath,['--input-type=module','-e',`import {packageComponents} from ${JSON.stringify(entry)};await packageComponents({root:${JSON.stringify(root)},args:${JSON.stringify(args)}});`],{encoding:'utf8',timeout:30000,env:{...process.env,PACK_REPORT:report}});}
test('exclusion packages public outputs without inspecting a malformed installed desktop',async t=>{
 const root=await fixture(t,{malformed:true});const run=execute(root,['--exclude-desktop']);assert.equal(run.status,0,run.stderr);
 assert.equal((await readFile(path.join(root,'result.txt'),'utf8')).trim(),'harness cli llama sdk-js sdk-python installers');
});
test('inclusion dispatches real desktop npm packaging and directory mode correctly',async t=>{
 const root=await fixture(t);let run=execute(root,['--installers-only','--include-desktop']);assert.equal(run.status,0,run.stderr);
 assert.equal((await readFile(path.join(root,'result.txt'),'utf8')).trim(),'installers desktop-installer');
 await writeFile(path.join(root,'result.txt'),'');run=execute(root,['--bundle-only','--include-desktop']);assert.equal(run.status,0,run.stderr);
 assert.equal((await readFile(path.join(root,'result.txt'),'utf8')).trim(),'bundle desktop-dir');
});
test('conflicting options fail before packaging and dry runs write no output',async t=>{
 const root=await fixture(t);const conflict=execute(root,['--include-desktop','--exclude-desktop']);assert.notEqual(conflict.status,0);assert.match(conflict.stderr,/not both/);await assert.rejects(access(path.join(root,'result.txt')));
 const dry=execute(root,['--include-desktop','--dry-run']);assert.equal(dry.status,0,dry.stderr);await assert.rejects(access(path.join(root,'result.txt')));
});
test('missing desktop supports exclusion and direct excluded app packaging is a no-op',async t=>{
 const root=await fixture(t,{desktop:false});assert.equal(execute(root,['--installers-only','--exclude-desktop']).status,0);
 await assert.rejects(packagingPlan({root,args:['--include-desktop']}),/desktop:clone/);
 assert.deepEqual((await packagingPlan({root,args:['--desktop-only','--exclude-desktop']})).steps,[]);
});

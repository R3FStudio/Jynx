import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,readFile,rm,access} from 'node:fs/promises';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import {pathToFileURL} from 'node:url';
import test from 'node:test';
import {inspectComponents} from '../workspace-components.mjs';
const entry=pathToFileURL(path.resolve(import.meta.dirname,'../build-all.ts')).href;
async function fixture(t,entries){
  const root=await mkdtemp(path.join(os.tmpdir(),'jynx-modular-build-'));t.after(()=>rm(root,{recursive:true,force:true}));
  const config={buildOrder:Object.keys(entries),packages:{}};const workspaces=[];
  for(const [id,item] of Object.entries(entries)){
    const folder=`components/${id}`;workspaces.push(folder);config.packages[id]={path:folder,name:`@fixture/${id}`,...(item.requires?{sourceDependencies:item.requires}:{})};
    if(item.missing)continue;
    await mkdir(path.join(root,folder),{recursive:true});
    await writeFile(path.join(root,folder,'package.json'),item.invalid?'bad JSON':JSON.stringify({name:`@fixture/${id}`,scripts:{build:'node build.mjs'}}));
    await writeFile(path.join(root,folder,'build.mjs'),item.fail?'process.exit(7);':`import {writeFileSync} from 'node:fs';writeFileSync(new URL('built.txt',import.meta.url),'built');`);
  }
  await writeFile(path.join(root,'package.json'),JSON.stringify({private:true,workspaces}));
  await writeFile(path.join(root,'workspace.config.json'),JSON.stringify(config));return root;
}
function build(root,exclude=[]){
  return spawnSync(process.execPath,['--input-type=module','-e',`import {buildAvailableComponents} from ${JSON.stringify(entry)};const result=await buildAvailableComponents({root:${JSON.stringify(root)},exclude:${JSON.stringify(exclude)}});console.log('BUILD_RESULT='+JSON.stringify(result));if(result.failed.length)process.exitCode=1;`],{encoding:'utf8',env:process.env,timeout:30000});
}
function result(run){return JSON.parse(run.stdout.split('\n').find(line=>line.startsWith('BUILD_RESULT=')).slice(13));}
test('missing desktop and harness do not block independent builds; CLI needs its source crates',async t=>{
  const root=await fixture(t,{desktop:{missing:true},harness:{missing:true},cli:{requires:['harness']},sdk:{}});const run=build(root);
  assert.equal(run.status,0,run.stderr);assert.deepEqual(result(run),{built:['sdk'],skipped:['desktop','harness','cli'],failed:[]});
  assert.equal(await readFile(path.join(root,'components/sdk/built.txt'),'utf8'),'built');
  await assert.rejects(access(path.join(root,'components/cli/built.txt')));assert.match(run.stdout,/Requires source component harness/);
});
test('a failed present build does not stop later components and still fails the overall command',async t=>{
  const root=await fixture(t,{first:{fail:true},second:{}});const run=build(root);
  assert.equal(run.status,1);assert.deepEqual(result(run),{built:['second'],skipped:[],failed:['first']});
  assert.equal(await readFile(path.join(root,'components/second/built.txt'),'utf8'),'built');
});
test('malformed neighbouring workspace metadata cannot prevent a valid package from building',async t=>{
  const root=await fixture(t,{broken:{invalid:true},healthy:{}});const run=build(root);
  assert.equal(run.status,1);assert.deepEqual(result(run),{built:['healthy'],skipped:[],failed:['broken']});
  assert.equal(await readFile(path.join(root,'components/healthy/built.txt'),'utf8'),'built');
});
test('an empty checkout of configured components succeeds without fabricating builds',async t=>{
  const root=await fixture(t,{desktop:{missing:true},sdk:{missing:true}});const run=build(root);
  assert.equal(run.status,0);assert.deepEqual(result(run),{built:[],skipped:['desktop','sdk'],failed:[]});
});
test('a present source component does not have to build a binary for its dependent to run',async t=>{
  const root=await fixture(t,{harness:{fail:true},cli:{requires:['harness']}});const run=build(root);
  assert.equal(run.status,1);assert.deepEqual(result(run),{built:['cli'],skipped:[],failed:['harness']});
});
test('unknown source dependencies are configuration errors rather than silently skipped work',async t=>{
  const root=await fixture(t,{sdk:{requires:['not-configured']}});const plan=await inspectComponents(root);
  assert.equal(plan.components[0].status,'invalid');assert.match(plan.components[0].reason,/sourceDependencies/);
});

 test('explicitly excluded desktop is not inspected or built even with malformed metadata',async t=>{
 const root=await fixture(t,{desktop:{invalid:true},harness:{}});const run=build(root,['desktop']);
 assert.equal(run.status,0,run.stderr);assert.deepEqual(result(run),{built:['harness'],skipped:['desktop'],failed:[]});
 });

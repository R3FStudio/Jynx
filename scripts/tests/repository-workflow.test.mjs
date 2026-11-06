import assert from 'node:assert/strict';
import {mkdtemp,mkdir,writeFile,copyFile,readFile,rm,rename,symlink} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import os from 'node:os';import path from 'node:path';import test from 'node:test';
import {setupRepositories,commitRepositories,assertPublicIndex,pushRepositories,verifyRepository,repositoryScopes,cloneLinkedRepository} from '../repositories.mjs';
const git=(cwd,...args)=>execFileSync('git',args,{cwd,encoding:'utf8',stdio:['ignore','pipe','pipe']}).trim();
async function fixture(t){
  const root=await mkdtemp(path.join(os.tmpdir(),'jynx-linked-git-'));t.after(()=>rm(root,{recursive:true,force:true}));
  await mkdir(path.join(root,'packages/private/Desktop'),{recursive:true});await mkdir(path.join(root,'scripts'));
  for(const name of ['repositories.mjs','private-remote.mjs'])await copyFile(path.resolve(import.meta.dirname,'../'+name),path.join(root,'scripts',name));
  await writeFile(path.join(root,'.gitignore'),'/packages/private/\n');await writeFile(path.join(root,'core.txt'),'PUBLIC_CANARY');
  await writeFile(path.join(root,'packages/private/Desktop/app.txt'),'PRIVATE_CANARY');
  await writeFile(path.join(root,'repositories.config.json'),JSON.stringify({public:{remote:'https://github.com/Example/Core.git',visibility:'public'},linked:[{id:'desktop',path:'packages/private/Desktop',remote:'https://github.com/Example/App.git',visibility:'private'}]}));
  await setupRepositories({root,log:()=>{}});
  for(const dir of [root,path.join(root,'packages/private/Desktop')]){git(dir,'config','user.name','Synthetic test');git(dir,'config','user.email','test@example.invalid');}
  return root;
}
test('ordinary root staging excludes private files; one command commits both repositories',async t=>{
  const root=await fixture(t);git(root,'add','.');assert.doesNotMatch(git(root,'diff','--cached','--name-only'),/packages\/private/);
  const result=await commitRepositories({root,message:'Synthetic update',log:()=>{}});assert.deepEqual(result.committed,['desktop','public']);
  assert.match(git(path.join(root,'packages/private/Desktop'),'ls-files'),/app.txt/);
  assert.doesNotMatch(git(root,'rev-list','--objects','--all'),/packages\/private|app.txt/);assertPublicIndex(root);
});
test('changes only in a linked checkout create no unnecessary public commit',async t=>{
  const root=await fixture(t);await commitRepositories({root,message:'Initial',log:()=>{}});const before=git(root,'rev-parse','HEAD');
  await writeFile(path.join(root,'packages/private/Desktop/app.txt'),'changed');
  const result=await commitRepositories({root,message:'Private update',log:()=>{}});assert.deepEqual(result,{committed:['desktop'],unchanged:['public']});assert.equal(git(root,'rev-parse','HEAD'),before);
});
test('a forced private index entry blocks the combined commit',async t=>{
  const root=await fixture(t);const blob=execFileSync('git',['hash-object','-w','--stdin'],{cwd:root,input:'PRIVATE_CANARY',encoding:'utf8'}).trim();
  git(root,'update-index','--add','--cacheinfo',`100644,${blob},packages/private/forced.txt`);
  assert.throws(()=>assertPublicIndex(root),/Commit blocked/);
  await assert.rejects(commitRepositories({root,message:'Must fail',log:()=>{}}),/Commit blocked/);
});
test('one push command handles both repositories without transferring private history',async t=>{
  const root=await fixture(t);await commitRepositories({root,message:'Initial',log:()=>{}});const scopes=await repositoryScopes(root);
  const temp=path.join(root,'remotes');await mkdir(temp);const emptyHooks=path.join(temp,'hooks');await mkdir(emptyHooks);
  for(const scope of scopes){const remote=path.join(temp,scope.id+'.git');git(root,'init','--bare',remote);git(scope.directory,'remote','set-url','origin',remote);}

  await writeFile(path.join(root,'.gitignore'),'/packages/private/\n/remotes/\n');await commitRepositories({root,message:'Ignore fixture remotes',log:()=>{}});
  const result=await pushRepositories({root,verify:async()=>{},runGit:(cwd,args)=>git(cwd,'-c',`core.hooksPath=${emptyHooks}`,...args),log:()=>{}});
  assert.deepEqual(result.pushed,['public','desktop']);
  assert.doesNotMatch(git(root,'--git-dir',path.join(temp,'public.git'),'rev-list','--objects','--all'),/app.txt|packages\/private/);
  assert.match(git(root,'--git-dir',path.join(temp,'desktop.git'),'ls-tree','-r','main'),/app.txt/);
});
test('private source requires a private destination while public source can target a private repository',async t=>{
  const root=await fixture(t);const scopes=await repositoryScopes(root);
  await verifyRepository(scopes[0],async()=>({ok:true,json:async()=>({private:true})}),()=>"synthetic-token");
  await assert.rejects(verifyRepository(scopes[1],async()=>({ok:true,json:async()=>({private:false})}),()=>"synthetic-token"),/visibility/);
});

test('clone installs app files at Desktop root and wires hooks without exposing them publicly',async t=>{
  const root=await fixture(t);await commitRepositories({root,message:'Initial',log:()=>{}});
  const installed=path.join(root,'packages/private/Desktop');const seed=path.join(root,'packages/private/seed');await rename(installed,seed);
  let count=0;
  const clone=async scope=>{count++;git(root,'clone','--',seed,scope.directory);git(scope.directory,'remote','set-url','origin',scope.remote);};
  await cloneLinkedRepository({root,id:'desktop',clone,log:()=>{}});
  assert.equal(await readFile(path.join(installed,'app.txt'),'utf8'),'PRIVATE_CANARY');
  assert.equal(git(installed,'rev-parse','--show-prefix'),'');
  assert.match(await readFile(path.join(installed,'git-hooks/pre-push'),'utf8'),/\.\.\/\.\.\/\.\.\/scripts\/repositories\.mjs/);
  await writeFile(path.join(installed,'app.txt'),'LOCAL_EDIT');
  await cloneLinkedRepository({root,id:'desktop',clone,log:()=>{}});
  assert.equal(count,1);assert.equal(await readFile(path.join(installed,'app.txt'),'utf8'),'LOCAL_EDIT');
  git(root,'add','.');assert.doesNotMatch(git(root,'diff','--cached','--name-only'),/packages\/private/);
});
test('clone refuses a non-repository directory and reports denied access without initializing a fake checkout',async t=>{
  const root=await fixture(t);const installed=path.join(root,'packages/private/Desktop');await rename(installed,path.join(root,'packages/private/seed'));
  await mkdir(installed);await writeFile(path.join(installed,'keep.txt'),'LOCAL_FILE');
  await assert.rejects(cloneLinkedRepository({root,id:'desktop',log:()=>{}}),/repo:setup/);
  assert.equal(await readFile(path.join(installed,'keep.txt'),'utf8'),'LOCAL_FILE');
  await rename(installed,path.join(root,'packages/private/non-repository'));
  await assert.rejects(cloneLinkedRepository({root,id:'desktop',clone:async()=>{throw new Error('access denied');},log:()=>{}}),/Authenticate Git/);
  await assert.rejects(readFile(path.join(installed,'.git/config')));
});

test('repository checks accept canonical aliases such as macOS temp links and Windows short paths',async t=>{
 const root=await fixture(t);const alias=root+'-alias';
 await symlink(root,alias,process.platform==='win32'?'junction':'dir');
 t.after(()=>rm(alias,{recursive:true,force:true}));
 await setupRepositories({root:alias,log:()=>{}});
 const scopes=await repositoryScopes(alias);assert.equal(scopes.length,2);
});

test('three checkouts use distinct commit messages and keep source in its own remote',async t=>{
 const root=await fixture(t);const file=path.join(root,'repositories.config.json');const config=JSON.parse(await readFile(file,'utf8'));
 config.linked.push({id:'code',path:'packages/private/jynx-code',remote:'https://github.com/Example/Code.git',visibility:'private'});
 await writeFile(file,JSON.stringify(config));const code=path.join(root,'packages/private/jynx-code');await mkdir(code);await writeFile(path.join(code,'extension.txt'),'EXTENSION_CANARY');
 await setupRepositories({root,log:()=>{}});git(code,'config','user.name','Synthetic test');git(code,'config','user.email','test@example.invalid');
 await assert.rejects(commitRepositories({root,messages:{public:'Core only'},log:()=>{}}),/provide a commit message/);
 await assert.rejects(commitRepositories({root,messages:{missing:'Unknown'},log:()=>{}}),/Unknown or uninstalled/);
 const result=await commitRepositories({root,messages:{public:'Improve the public harness',desktop:'Connect desktop models',code:'Introduce Jynx Code'},log:()=>{}});
 assert.deepEqual(result.committed,['desktop','code','public']);
 assert.equal(git(root,'log','-1','--format=%s'),'Improve the public harness');
 assert.equal(git(code,'log','-1','--format=%s'),'Introduce Jynx Code');
 assert.doesNotMatch(git(root,'rev-list','--objects','--all'),/extension.txt|packages\/private/);
 const scope=(await repositoryScopes(root)).find(scope=>scope.id==='code');
 await assert.rejects(verifyRepository(scope,async()=>({ok:true,json:async()=>({private:false})}),()=> 'synthetic'),/visibility/);
});

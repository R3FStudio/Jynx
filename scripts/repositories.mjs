import {realpathSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {access,chmod,mkdir,readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {githubRepository,githubCredential,repositoryMetadata} from './private-remote.mjs';
export const projectRoot=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const exists=file=>access(file).then(()=>true,()=>false);
export function git(directory,args,input){return execFileSync('git',args,{cwd:directory,input,encoding:'utf8',stdio:[input===undefined?'ignore':'pipe','pipe','pipe'],maxBuffer:16*1024*1024}).trim();}
const normalize=value=>{const real=realpathSync.native(path.resolve(value));return process.platform==='win32'?real.toLowerCase():real;};
export async function repositoryScopes(root=projectRoot){
  const config=JSON.parse(await readFile(path.join(root,'repositories.config.json'),'utf8'));
  const scopes=[{id:'public',directory:root,...config.public}];
  for(const entry of config.linked??[]){
    const directory=path.resolve(root,entry.path);const relative=path.relative(root,directory);
    if(!relative||relative.startsWith('..')||path.isAbsolute(relative)||!relative.startsWith(`packages${path.sep}private${path.sep}`)&&relative!==`packages${path.sep}private`)throw new Error('Linked repositories must be inside the ignored packages/private directory.');
    scopes.push({...entry,directory});
  }
  for(const scope of scopes){githubRepository(scope.remote);if(scope.visibility!==(scope.id==='public'?'public':'private'))throw new Error('Root visibility must be public and linked repository visibility must be private.');}
  return scopes;
}
function requireOwnRepository(scope){
  let top='';try{top=git(scope.directory,['rev-parse','--show-toplevel']);}catch{}
  if(!top||normalize(top)!==normalize(scope.directory))throw new Error(`${scope.id}: run npm run repo:setup before committing this checkout.`);
}
export function assertPublicIndex(root=projectRoot){
  const forbidden=value=>/^(packages\/private|private|packages\/desktop)(?:\/|$)/i.test(value);
  const staged=git(root,['diff','--cached','--name-only','-z']).split('\0').filter(Boolean);
  if(staged.some(forbidden))throw new Error('Commit blocked: ignored application files are staged in the public repository. Unstage them before continuing.');
  let head='';try{head=git(root,['rev-parse','--verify','HEAD']);}catch{}
  if(head){
    const paths=git(root,['rev-list','--objects','--all']).split('\n').map(line=>line.replace(/^[0-9a-f]+ /,''));
    if(paths.some(forbidden))throw new Error('Public history contains private paths. Publishing is blocked; existing history has not been rewritten.');
  }
}
export async function setupRepositories({root=projectRoot,clone=false,log=console.log}={}){
  const scopes=await repositoryScopes(root);
  for(const scope of scopes){
    if(scope.id!=='public'&&!await exists(scope.directory)){
      if(!clone){log(`${scope.id}: checkout not installed; skipped.`);continue;}
      await mkdir(path.dirname(scope.directory),{recursive:true});git(root,['clone','--',scope.remote,scope.directory]);
    }
    let own=false;try{own=normalize(git(scope.directory,['rev-parse','--show-toplevel']))===normalize(scope.directory);}catch{}
    if(!own)git(scope.directory,['init','--initial-branch=main']);
    requireOwnRepository(scope);
    let current='';try{current=git(scope.directory,['remote','get-url','origin']);}catch{}
    if(current&&current!==scope.remote){
      if(scope.id!=='public'||!scopes.slice(1).some(entry=>entry.remote===current))throw new Error(`${scope.id}: origin points elsewhere and was not changed.`);
      git(scope.directory,['remote','set-url','origin',scope.remote]);
    }else if(!current)git(scope.directory,['remote','add','origin',scope.remote]);
    const hooks=scope.id==='public'?'.githooks':'git-hooks';await mkdir(path.join(scope.directory,hooks),{recursive:true});
    const runner=path.relative(scope.directory,path.join(root,'scripts/repositories.mjs')).split(path.sep).join('/');
    const push=path.join(scope.directory,hooks,'pre-push');await writeFile(push,`#!/bin/sh\nif [ -f "${runner}" ]; then\n  exec node "${runner}" guard ${scope.id} "$2"\nfi\nif [ -f "scripts/check-remote.mjs" ]; then\n  exec node scripts/check-remote.mjs "$2"\nfi\necho "Repository coordinator is unavailable; run repo:setup in the parent workspace." >&2\nexit 1\n`);await chmod(push,0o755);
    if(scope.id==='public'){
      const commit=path.join(scope.directory,hooks,'pre-commit');await writeFile(commit,'#!/bin/sh\nexec node scripts/repositories.mjs guard-index\n');await chmod(commit,0o755);
    }
    git(scope.directory,['config','core.hooksPath',hooks]);log(`${scope.id}: repository and push guard ready.`);
  }
  return scopes;
}
export async function cloneLinkedRepository({root=projectRoot,id,clone,log=console.log}={}){
  const scope=(await repositoryScopes(root)).find(scope=>scope.id===id&&scope.id!=='public');
  if(!scope)throw new Error('Unknown linked repository.');
  if(await exists(scope.directory)){
    requireOwnRepository(scope);
    const origin=git(scope.directory,['remote','get-url','origin']);
    if(githubRepository(origin).toLowerCase()!==githubRepository(scope.remote).toLowerCase())throw new Error(`${id}: installed checkout points to a different remote; no files were changed.`);
    log(`${id}: already installed at ${path.relative(root,scope.directory)}; keeping its current changes.`);
  }else{
    await mkdir(path.dirname(scope.directory),{recursive:true});
    try{
      if(clone)await clone(scope);
      else git(root,['clone','--',scope.remote,scope.directory]);
    }catch(error){throw new Error(`${id}: clone failed. Authenticate Git with access to ${scope.remote} and retry. ${String(error.message).split('\n')[0]}`);}
    requireOwnRepository(scope);
    log(`${id}: cloned into ${path.relative(root,scope.directory)}.`);
  }
  await setupRepositories({root,log});
  return scope;
}
async function installedScopes(root){
  const scopes=await repositoryScopes(root);const result=[];
  for(const scope of scopes){if(!await exists(scope.directory))continue;requireOwnRepository(scope);result.push(scope);}return result;
}
export async function commitRepositories({root=projectRoot,message,messages={},log=console.log}={}){
  if((typeof message!=='string'||!message.trim())&&!Object.values(messages).some(value=>typeof value==='string'&&value.trim()))throw new Error('Provide a commit message: npm run repo:commit -- "Your message"');
  const scopes=await installedScopes(root);assertPublicIndex(root);
  for(const id of Object.keys(messages))if(!scopes.some(scope=>scope.id===id))throw new Error(`Unknown or uninstalled repository: ${id}`);
  for(const scope of scopes)if(!(messages[scope.id]||message||'').trim())throw new Error(`${scope.id}: provide a commit message before committing any repository.`);
  for(const scope of scopes)if(git(scope.directory,['diff','--name-only','--diff-filter=U']))throw new Error(`${scope.id}: resolve merge conflicts before committing.`);
  const committed=[],unchanged=[];

  for(const scope of [...scopes.filter(scope=>scope.id!=='public'),...scopes.filter(scope=>scope.id==='public')]){
    git(scope.directory,['add','--all','--','.']);
    if(scope.id==='public')assertPublicIndex(root);
    let changed=true;try{git(scope.directory,['diff','--cached','--quiet']);changed=false;}catch{}
    if(!changed){unchanged.push(scope.id);log(`${scope.id}: no changes.`);continue;}
    git(scope.directory,['commit','--quiet','-m',messages[scope.id]||message]);committed.push(scope.id);log(`${scope.id}: committed.`);
  }
  return {committed,unchanged};
}
export async function verifyRepository(scope,fetcher=fetch,tokenProvider=githubCredential){
  const configured=githubRepository(scope.remote);const actual=githubRepository(git(scope.directory,['remote','get-url','origin']));
  if(configured.toLowerCase()!==actual.toLowerCase())throw new Error(`${scope.id}: origin differs from repositories.config.json.`);
  const token=tokenProvider(configured);const metadata=await repositoryMetadata(configured,token,fetcher);
  if(typeof metadata.private!=='boolean'||(scope.visibility==='private'&&metadata.private!==true))throw new Error(`${scope.id}: remote visibility does not match ${scope.visibility}. Push blocked.`);
}
export async function pushRepositories({root=projectRoot,verify=verifyRepository,runGit=git,log=console.log}={}){
  const scopes=await installedScopes(root);assertPublicIndex(root);const ready=[];
  for(const scope of scopes){
    if(git(scope.directory,['status','--porcelain','--untracked-files=normal']))throw new Error(`${scope.id}: commit changes before pushing.`);
    let head='';try{head=git(scope.directory,['rev-parse','--verify','HEAD']);}catch{}
    if(!head){log(`${scope.id}: no commits to push.`);continue;}
    await verify(scope);const branch=git(scope.directory,['symbolic-ref','--short','HEAD']);ready.push({scope,branch});
  }
  const pushed=[];

  for(const {scope,branch} of ready){
    try{runGit(scope.directory,['push','--set-upstream','origin',branch]);pushed.push(scope.id);log(`${scope.id}: pushed ${branch}.`);}
    catch(error){throw new Error(`${scope.id}: push failed. Repositories already pushed: ${pushed.join(', ')||'none'}. ${String(error.message).split('\n')[0]}`);}
  }
  return {pushed};
}
async function main(){
  const [command,...args]=process.argv.slice(2);
  if(command==='setup')return setupRepositories({clone:args.includes('--clone')});
  if(command==='clone')return cloneLinkedRepository({id:args[0]});
  if(command==='guard-index')return assertPublicIndex();
  if(command==='guard'){
    const scope=(await repositoryScopes()).find(scope=>scope.id===args[0]);if(!scope)throw new Error('Unknown repository scope.');
    if(args[1]&&githubRepository(args[1]).toLowerCase()!==githubRepository(scope.remote).toLowerCase())throw new Error('Push destination differs from configured origin.');
    if(scope.id==='public')assertPublicIndex();return verifyRepository(scope);
  }
  if(command==='status'){
    for(const scope of await installedScopes(projectRoot)){console.log(`${scope.id}:`);console.log(git(scope.directory,['status','--short'])||'clean');}return;
  }
  if(command==='commit'||command==='sync'){
    const messages={};const shared=[];
    for(let index=0;index<args.length;index++){
      if(args[index]==='--message'){
        const id=args[++index],value=args[++index];if(!id||!value)throw new Error('Use --message <repository id> "Commit message".');messages[id]=value;
      }else shared.push(args[index]);
    }
    await commitRepositories({message:shared.join(' '),messages});if(command==='sync')await pushRepositories();return;
  }
  if(command==='push')return pushRepositories();
  throw new Error('Use setup, clone, status, commit, push, or sync.');
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){main().catch(error=>{console.error(error.message);process.exitCode=1;});}

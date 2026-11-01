import {readFile,writeFile,access} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
export const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const exists=p=>access(p).then(()=>true,()=>false);
export async function synchronizeVersion({directory=root,version,write=false}={}){
  const rootFile=path.join(directory,'package.json');const pkg=JSON.parse(await readFile(rootFile,'utf8'));
  const wanted=version??pkg.version;
  if(!/^\d+\.\d+\.\d+$/.test(wanted))throw new Error('Use a stable semantic version, for example 0.0.2.');
  const config=JSON.parse(await readFile(path.join(directory,'workspace.config.json'),'utf8'));
  const manifests=[rootFile,...Object.values(config.packages).map(p=>path.join(directory,p.path,'package.json'))];
  const mismatches=[];
  async function save(file,old,next){if(old!==next){mismatches.push(path.relative(directory,file).split(path.sep).join('/'));if(write)await writeFile(file,next);}}
  for(const file of new Set(manifests)){
    if(!await exists(file))continue;const old=await readFile(file,'utf8');const data=JSON.parse(old);
    if(data.version!==wanted){data.version=wanted;await save(file,old,JSON.stringify(data,null,2)+'\n');}
  }
  for(const relative of ['packages/harness/Cargo.toml','packages/cli/Cargo.toml','packages/sdk-python/pyproject.toml']){
    const file=path.join(directory,relative);if(!await exists(file))continue;const old=await readFile(file,'utf8');
    await save(file,old,old.replace(/^(version\s*=\s*")[^"]+("\s*)$/m,`$1${wanted}$2`));
  }
  const api=path.join(directory,'api/local-model.openapi.yaml');if(await exists(api)){const old=await readFile(api,'utf8');await save(api,old,old.replace(/^(\s*version:\s*)[^\r\n]+/m,`$1${wanted}`));}
  for(const relative of ['package-lock.json','packages/private/Desktop/package-lock.json']){
    const file=path.join(directory,relative);if(!await exists(file))continue;const old=await readFile(file,'utf8');const data=JSON.parse(old);let changed=data.version!==wanted;data.version=wanted;
    for(const [key,value] of Object.entries(data.packages??{})){
      if(key===''||((key.startsWith('packages/')||key.startsWith('tools/'))&&!key.includes('/node_modules/'))){if(value.version!==wanted){value.version=wanted;changed=true;}}
    }
    if(changed)await save(file,old,JSON.stringify(data,null,2)+'\n');
  }
  for(const relative of ['packages/harness/Cargo.lock','packages/cli/Cargo.lock']){
    const file=path.join(directory,relative);if(!await exists(file))continue;const old=await readFile(file,'utf8');
    const next=old.replace(/\[\[package\]\]\r?\nname = "jynx-[^"]+"\r?\nversion = "[^"]+"(?=\r?\n(?!source =))/g,block=>block.replace(/version = "[^"]+"/,`version = "${wanted}"`));
    await save(file,old,next);
  }
  return {version:wanted,mismatches};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
 try{
  const command=process.argv[2];if(!['set','check'].includes(command))throw new Error('Use npm run version:set -- 0.0.2 or npm run version:check.');
  if(command==='set'&&!process.argv[3])throw new Error('Supply a version.');
  const result=await synchronizeVersion({version:command==='set'?process.argv[3]:undefined,write:command==='set'});
  if(command==='check'&&result.mismatches.length)throw new Error('Version drift: '+result.mismatches.join(', ')+'. Run npm run version:set -- '+result.version);
  console.log(command==='set'?`Version ${result.version}: synchronized ${result.mismatches.length} files. Review, test, and commit both repositories.`:`All installed components match ${result.version}.`);
 }catch(error){console.error(error.message);process.exitCode=1;}
}

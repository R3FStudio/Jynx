import {readFile,readdir,access} from 'node:fs/promises';import path from 'node:path';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');const files=new Set();
async function collect(directory){
 let entries;try{entries=await readdir(directory,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return;throw error;}
 for(const entry of entries){const file=path.join(directory,entry.name);if(entry.isDirectory())await collect(file);else if(entry.isFile()&&entry.name.endsWith('.md'))files.add(file);}
}
for(const name of ['README.md','CONTRIBUTING.md','ARCHITECTURE.md','assets/README.md'])files.add(path.join(root,name));
await collect(path.join(root,'docs'));
const config=JSON.parse(await readFile(path.join(root,'workspace.config.json'),'utf8'));
for(const component of Object.values(config.packages)){
 const directory=path.join(root,component.path);try{await access(path.join(directory,'README.md'));files.add(path.join(directory,'README.md'));}catch{}
 await collect(path.join(directory,'docs'));
}
const problems=[];
for(const file of files){
 const text=await readFile(file,'utf8');
 for(const match of text.matchAll(/\]\(([^)]+)\)|(?:src|href)="([^"]+)"/g)){
  const target=(match[1]||match[2]).split('#')[0].split('?')[0];if(!target||/^[a-z][a-z0-9+.-]*:/i.test(target))continue;
  const resolved=target.startsWith('/')?path.resolve(root,'.'+target):path.resolve(path.dirname(file),target);
  try{await access(decodeURIComponent(resolved));}catch{problems.push(`${path.relative(root,file)}: missing ${target}`);}
 }
}
if(problems.length){console.error(problems.join('\n'));process.exitCode=1;}else console.log(`Documentation links passed (${files.size} files).`);

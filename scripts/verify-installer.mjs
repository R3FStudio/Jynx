import path from 'node:path';import {fileURLToPath} from 'node:url';
import {mkdir,readFile,access} from 'node:fs/promises';import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const platform=process.platform==='win32'?'windows':process.platform==='darwin'?'macos':'linux';const arch=process.arch;
const version=JSON.parse(await readFile(path.join(root,'package.json'),'utf8')).version;
const temp=path.join(root,'.cache','installer-smoke',platform+'-'+arch+'-'+process.pid);await mkdir(temp,{recursive:true});
const prefix=path.join(temp,'Jynx install');const bin=path.join(temp,'commands');
const environment={...process.env,JYNX_STATE_DIR:path.join(temp,'state'),JYNX_MCP_CONFIG:path.join(temp,'mcp.json'),JYNX_SKILLS_DIR:path.join(temp,'skills'),JYNX_NO_UPDATE_CHECK:'1'};
function run(executable,args){return execFileSync(executable,args,{cwd:temp,env:environment,encoding:'utf8',windowsHide:true,stdio:['ignore','pipe','pipe']});}
const setup=path.join(root,'dist',`Jynx-Complete-Setup-v${version}-${platform}-${arch}.${platform==='windows'?'exe':platform==='macos'?'pkg':'run'}`);
if(platform==='windows')execFileSync(setup,['/S','/TESTMODE',`/D=${prefix}`],{cwd:temp,env:environment,windowsHide:true,windowsVerbatimArguments:true,stdio:'pipe'});
else if(platform==='linux')run('sh',[setup,'--prefix',prefix,'--bin-dir',bin]);
else{
 run('pkgutil',['--expand',setup,path.join(temp,'expanded-pkg')]);
 run('sh',[path.join(root,'.cache/public-installers',platform+'-'+arch,'payload/install.sh'),'--prefix',prefix,'--bin-dir',bin]);
}
const ext=platform==='windows'?'.exe':'';
for(const kind of ['cli','harness']){
 const output=run(path.join(prefix,'bin','jynx-'+kind+ext),['--version']);if(!output.includes(version))throw new Error('Installed version mismatch: '+output);console.log(output.trim());
}
const manifest=JSON.parse(await readFile(path.join(prefix,'runtime-manifest.json'),'utf8'));
const cpu=manifest.runtimes.find(runtime=>runtime.id==='cpu');
const runtime=run(path.join(prefix,cpu.server),['--version']);console.log('Bundled llama.cpp: '+cpu.release+' '+runtime.trim().split('\n')[0]);
await access(path.join(prefix,'source/jynx-source.tar.gz'));
if(platform==='windows')run(path.join(prefix,'Uninstall.exe'),['/S']);
else run('sh',[path.join(prefix,'uninstall.sh')]);
let removed=false;for(let attempt=0;attempt<50;attempt++){try{await access(path.join(prefix,'bin','jynx-cli'+ext));}catch{removed=true;break;}await new Promise(resolve=>setTimeout(resolve,100));}
if(!removed)throw new Error('Installer uninstall left the executable behind');
console.log('Installer install, versions, bundled runtime, source inclusion, and uninstall passed.');

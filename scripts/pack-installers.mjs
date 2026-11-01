import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {access,chmod,copyFile,cp,mkdir,readFile,readdir,rm,writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {create as createTar} from 'tar';
import {defaultLlamaRuntime} from './llama-runtime-targets.ts';
import {privacyText,privacyHtml} from './privacy-assets.mjs';
const workspace=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const host=process.platform==='win32'?'windows':process.platform==='darwin'?'macos':'linux';
const exists=file=>access(file).then(()=>true,()=>false);
const hash=data=>createHash('sha256').update(data).digest('hex');
function execute(command,args,cwd){execFileSync(command,args,{cwd,stdio:'inherit',windowsHide:true});}
export function inside(directory,relative){
 if(typeof relative!=='string'||!relative||path.isAbsolute(relative)||/^[a-z]:/i.test(relative)||relative.split(/[\\/]/).some(part=>part==='..'))throw new Error('Unsafe bundle path: '+relative);
 const target=path.resolve(directory,relative);const diff=path.relative(directory,target);
 if(!diff||diff.startsWith('..')||path.isAbsolute(diff))throw new Error('Unsafe bundle path: '+relative);
 return target;
}
async function reset(directory,base){const relative=path.relative(base,directory);if(!relative||relative.startsWith('..')||path.isAbsolute(relative))throw new Error('Invalid staging directory');await rm(directory,{recursive:true,force:true});await mkdir(directory,{recursive:true});}
async function filesIn(root){const files=[];async function visit(dir){for(const entry of await readdir(dir,{withFileTypes:true})){const file=path.join(dir,entry.name);if(entry.isDirectory())await visit(file);else if(entry.isFile())files.push(path.relative(root,file).split(path.sep).join('/'));}}await visit(root);return files.sort();}
export async function stageBundle({root=workspace,platform=host,arch=process.arch}={}){
 if(!['windows','linux','macos'].includes(platform)||!['x64','arm64'].includes(arch))throw new Error('Unsupported installer target');
 const version=JSON.parse(await readFile(path.join(root,'package.json'),'utf8')).version;
 if(!/^\d+\.\d+\.\d+$/.test(version))throw new Error('Installer requires a stable release version');
 const base=path.join(root,'.cache','public-installers');const staging=path.join(base,platform+'-'+arch);await reset(staging,base);
 const payload=path.join(staging,'payload');await mkdir(path.join(payload,'bin'),{recursive:true});
 const ext=platform==='windows'?'.exe':'';
 for(const name of ['cli','harness']){
  const file=`jynx-${name}${ext}`;await copyFile(path.join(root,'packages',name,'target/release',file),path.join(payload,'bin',file));await chmod(path.join(payload,'bin',file),0o755);
 }
 const binaries=path.join(root,'vendor/llama.cpp/binaries');const upstream=JSON.parse(await readFile(path.join(binaries,'manifest.json'),'utf8'));
 const backends=[...new Set(['cpu',defaultLlamaRuntime(platform,arch)])];const runtimes=[];
 for(const backend of backends){
  const key=`${platform}/${arch}/${backend}`;const record=upstream.runtimes?.[key];if(!record?.server)throw new Error('Missing synced runtime '+key+'; run npm run llama:sync -- --portable');
  const source=path.join(binaries,platform,arch,backend);const server=inside(source,record.server);await access(server);
  const target=path.join(payload,'runtimes',backend);await cp(source,target,{recursive:true,dereference:true});
  const relative=`runtimes/${backend}/${record.server.replaceAll('\\','/')}`;await chmod(inside(payload,relative),0o755);
  runtimes.push({id:backend,server:relative,release:record.release,commit:record.commit,assets:record.assets??[]});
 }
 await writeFile(path.join(payload,'runtime-manifest.json'),JSON.stringify({format:1,defaultRuntime:'cpu',platform,arch,runtimes},null,2)+'\n');
 await mkdir(path.join(payload,'licenses/llama.cpp'),{recursive:true});await copyFile(path.join(root,'vendor/llama.cpp/source/LICENSE'),path.join(payload,'licenses/llama.cpp/LICENSE'));
 await mkdir(path.join(payload,'licenses/jynx-artwork'),{recursive:true});
 await copyFile(path.join(root,'assets/branding/LICENSE.txt'),path.join(payload,'licenses/jynx-artwork/LICENSE.txt'));
 for(const name of ['LICENSE','COPYRIGHT','PRIVACY.md'])await copyFile(path.join(root,name),path.join(payload,name));
 const policy=await readFile(path.join(root,'PRIVACY.md'),'utf8');
 await writeFile(path.join(payload,'PRIVACY.txt'),privacyText(policy));
 const macResources=path.join(staging,'mac-resources');await mkdir(macResources,{recursive:true});
 await writeFile(path.join(macResources,'Privacy.html'),privacyHtml(policy));
 await mkdir(path.join(payload,'setup'),{recursive:true});await copyFile(path.join(root,'assets/branding/jynx.ico'),path.join(payload,'setup/jynx.ico'));await copyFile(path.join(workspace,'scripts/installers/windows-path.ps1'),path.join(payload,'setup/windows-path.ps1'));
 for(const name of ['install.sh','uninstall.sh']){await writeFile(path.join(payload,name),(await readFile(path.join(workspace,'scripts/installers',name),'utf8')).replaceAll('\r\n','\n'));await chmod(path.join(payload,name),0o755);}
 const server=runtimes.find(runtime=>runtime.id==='cpu').server;
 const launcher=platform==='windows'?`@echo off\r\n"%~dp0..\\${server.replaceAll('/','\\')}" -ngl 0 %*\r\n`:`#!/bin/sh\nset -eu\ntarget=$0\nwhile [ -L "$target" ]; do link=$(readlink "$target"); case "$link" in /*) target=$link ;; *) target=$(dirname -- "$target")/$link ;; esac; done\nroot=$(CDPATH= cd -- "$(dirname -- "$target")/.." && pwd)\nexec "$root/${server}" -ngl 0 "$@"\n`;
 await writeFile(path.join(payload,'bin','jynx-llama'+(platform==='windows'?'.cmd':'')),launcher);if(platform!=='windows')await chmod(path.join(payload,'bin/jynx-llama'),0o755);
 await mkdir(path.join(payload,'source'),{recursive:true});
 const sourceFiles=execFileSync('git',['ls-files','--cached','--others','--exclude-standard','-z'],{cwd:root,encoding:'utf8'}).split('\0').filter(Boolean);
 if(sourceFiles.some(file=>/^(packages\/private|private)(?:\/|$)/i.test(file)))throw new Error('Private paths must never enter the public source archive');
 await createTar({cwd:root,file:path.join(payload,'source/jynx-source.tar.gz'),gzip:true,portable:true},[...new Set(sourceFiles)]);
 await writeFile(path.join(payload,'README.txt'),`Jynx ${version} - Your Coding Agent.\n\nIncludes Jynx CLI, Jynx Harness, and llama.cpp (${backends.join(', ')}).\nModel weights are not included.\n\nCommands: jynx-cli, jynx-harness, jynx-llama.\nLocal model: jynx-cli chat --gguf /path/to/model.gguf\nBackend: add --runtime ${backends.find(x=>x!=='cpu')||'cpu'} to select another installed backend.\n\nThe harness is a JSON-line stdio server, not a background service installed at startup.\nOpen a new terminal after setup.\nUnix portable install: sh install.sh [--prefix /absolute/path] [--bin-dir /absolute/path]\nUnix removal: sh /path/to/installation/uninstall.sh\nSource snapshot: source/jynx-source.tar.gz\nDocumentation: https://github.com/R3FStudio/Jynx/tree/main/docs\nPrivacy: PRIVACY.md or https://github.com/R3FStudio/Jynx/blob/main/PRIVACY.md\n\nGPL-3.0-only applies to Jynx. llama.cpp retains its MIT license under licenses/llama.cpp.\n`);
 const files=await filesIn(payload);const checksums={};for(const file of files)checksums[file]=hash(await readFile(path.join(payload,file)));
 await writeFile(path.join(payload,'install-manifest.json'),JSON.stringify({product:'Jynx',version,platform,arch,backends,files:checksums},null,2)+'\n');
 return {root,version,platform,arch,staging,payload};
}
async function checksum(file){await writeFile(file+'.sha256',`${hash(await readFile(file))}  ${path.basename(file)}\n`);}
export async function ensureNsis(root=workspace){
 const candidates=[process.env.MAKENSIS,path.join(process.env['ProgramFiles(x86)']||'C:/Program Files (x86)','NSIS/makensis.exe')].filter(Boolean);
 for(const candidate of candidates)if(await exists(candidate))return candidate;
 const folder=path.join(root,'.cache/build-tools/nsis-3.12');const compiler=path.join(folder,'nsis-3.12/makensis.exe');if(await exists(compiler))return compiler;
 await mkdir(folder,{recursive:true});const zip=path.join(folder,'nsis.zip');
 const response=await fetch('https://downloads.sourceforge.net/project/nsis/NSIS%203/3.12/nsis-3.12.zip',{signal:AbortSignal.timeout(60000)});if(!response.ok)throw new Error('NSIS download failed: '+response.status);
 const data=Buffer.from(await response.arrayBuffer());if(hash(data)!=='56581f90db321581c5381193d796fffcf2d24b2f8fed2160a6c6a3baa67f2c4f')throw new Error('NSIS compiler checksum mismatch');
 await writeFile(zip,data);execute('tar',['-xf',zip,'-C',folder],root);await access(compiler);return compiler;
}
export async function packageBundle(bundle,{archiveOnly=false}={}){
 const {root,version,platform,arch,payload,staging}=bundle;const output=path.join(root,'dist');await mkdir(output,{recursive:true});
 const suffix=`v${version}-${platform}-${arch}`;const base=`Jynx-Complete-Setup-${suffix}`;const archive=path.join(output,`Jynx-Complete-Portable-${suffix}.tar.gz`);
 await createTar({cwd:payload,file:archive,gzip:true,portable:true},await readdir(payload));await checksum(archive);
 if(archiveOnly)return {archive};let installer;
 if(platform==='windows'){
  if(process.platform!=='win32')throw new Error('Compile Windows installers on Windows');
  installer=path.join(output,base+'.exe');const compiler=await ensureNsis(root);
  execute(compiler,[`/DVERSION=${version}`,`/DPAYLOAD=${payload}`,`/DOUTPUT=${installer}`,path.join(workspace,'scripts/installers/windows.nsi')],root);
 }else if(platform==='linux'){
  const template=(await readFile(path.join(workspace,'scripts/installers/linux-run.sh'),'utf8')).replaceAll('\r\n','\n');installer=path.join(output,base+'.run');
  await writeFile(installer,Buffer.concat([Buffer.from(template.replaceAll('@ARCH@',arch)),await readFile(archive)]));await chmod(installer,0o755);
 }else{
  if(process.platform!=='darwin')throw new Error('Compile macOS packages on macOS');
  const pkgRoot=path.join(staging,'pkg-root');const installRoot=path.join(pkgRoot,'usr/local/lib/jynx');await mkdir(path.dirname(installRoot),{recursive:true});await cp(payload,installRoot,{recursive:true});
  const bin=path.join(pkgRoot,'usr/local/bin');await mkdir(bin,{recursive:true});const {symlink}=await import('node:fs/promises');
  for(const name of ['jynx-cli','jynx-harness','jynx-llama'])await symlink('/usr/local/lib/jynx/bin/'+name,path.join(bin,name));
  installer=path.join(output,base+'.pkg');
  const component=path.join(staging,'Jynx-component.pkg');
  execute('pkgbuild',['--root',pkgRoot,'--identifier','org.r3fstudio.jynx','--version',version,'--install-location','/','--ownership','recommended',component],root);
  const distribution=path.join(staging,'Distribution.xml');
  execute('productbuild',['--synthesize','--package',component,distribution],root);
  const xml=await readFile(distribution,'utf8');
  if(!/<installer-gui-script\b[^>]*>/.test(xml))throw new Error('macOS package distribution has no installer root');
  await writeFile(distribution,xml.replace(/(<installer-gui-script\b[^>]*>)/,'$1\n<readme file="Privacy.html" mime-type="text/html"/>'));
  const args=['--distribution',distribution,'--resources',path.join(staging,'mac-resources'),'--package-path',staging];
  if(process.env.JYNX_MAC_INSTALLER_IDENTITY)args.push('--sign',process.env.JYNX_MAC_INSTALLER_IDENTITY);
  execute('productbuild',[...args,installer],root);
 }
 await checksum(installer);return {archive,installer};
}
async function main(){
 const root=workspace;const arch=process.arch;const platform=host;
 const manifest=path.join(root,'vendor/llama.cpp/binaries/manifest.json');let ready=false;
 try{const metadata=JSON.parse(await readFile(manifest,'utf8'));ready=['cpu',defaultLlamaRuntime(platform,arch)].every(backend=>metadata.runtimes?.[`${platform}/${arch}/${backend}`]);}catch{}
 if(!ready)execute(process.execPath,[path.join(root,'scripts/sync-llama.ts'),'--platform',platform,'--arch',arch,'--portable'],root);
 const bundle=await stageBundle({root,platform,arch});const result=await packageBundle(bundle,{archiveOnly:process.argv.includes('--archive-only')});
 console.log('Public artifacts:',result);
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href)main().catch(error=>{console.error(error.message);process.exitCode=1;});

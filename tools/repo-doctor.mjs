import {access,readFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {inspectComponents} from '../scripts/workspace-components.mjs';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const exists=async file=>access(path.join(root,file)).then(()=>true,()=>false);
const {config,rootPackage,components}=await inspectComponents(root);
const problems=components.filter(item=>item.status==='invalid').map(item=>`${item.id}: ${item.reason}`);
const available=components.filter(item=>item.status==='ready');
const skipped=components.filter(item=>item.status==='missing'||item.status==='unavailable').map(item=>({id:item.id,reason:item.reason}));
const commands={desktop:['start:desktop','pack:desktop','package:desktop'],harness:['start:harness','pack:harness'],cli:['start:cli','pack:cli'],'sdk-js':['pack:sdk-js'],'sdk-python':['pack:sdk-python'],tooling:['lint']};
const files={
  desktop:['src/main/main.ts','src/renderer/components/root/App.tsx','vite.config.ts','tsconfig.main.json','tsconfig.renderer.json'],
  harness:['Cargo.toml','Cargo.lock','rust-toolchain.toml','server/main.rs','server/Cargo.toml'],
  cli:['Cargo.toml','Cargo.lock','rust-toolchain.toml'],
  'sdk-js':['src/index.js'],'sdk-python':['src/jynx_sdk/__init__.py'],
};
for(const component of available){
  for(const command of commands[component.id]??[])if(!rootPackage.scripts?.[command])problems.push(`Missing root command ${command}`);
  if(['desktop','harness','cli'].includes(component.id)&&!component.manifest.scripts?.start)problems.push(`${component.path}/package.json needs a start script`);
  for(const file of files[component.id]??[])if(!await exists(`${component.path}/${file}`))problems.push(`Missing ${component.path}/${file}`);
  if(component.id==='harness'&&await exists(`${component.path}/Cargo.toml`)){
    const cargo=await readFile(path.join(component.directory,'Cargo.toml'),'utf8');
    if(/^\s*"cli"\s*,?\s*$/m.test(cargo))problems.push('CLI must build outside the Harness Cargo workspace');
    if(await exists(`${component.path}/server/Cargo.toml`)){
      const server=await readFile(path.join(component.directory,'server/Cargo.toml'),'utf8');
      if(!server.includes('name = "jynx-harness"'))problems.push('Harness server binary is missing from its Cargo manifest');
    }
  }
  if(component.id==='cli'&&await exists(`${component.path}/Cargo.toml`)){
    const cargo=await readFile(path.join(component.directory,'Cargo.toml'),'utf8');
    if(!/^\[workspace\]$/m.test(cargo))problems.push('CLI needs an independent Cargo workspace');
    if(!cargo.includes('../harness/'))problems.push('CLI should consume shared Harness crates by path dependency');
  }
}
for(const workspace of rootPackage.workspaces??[]){
  if(await exists(`${workspace}/package.json`)&&!Object.values(config.packages).some(entry=>entry.path===workspace))problems.push(`${workspace} is missing from workspace.config.json`);
}
if(await exists('api')&&!await exists('api/local-model.openapi.yaml'))problems.push('Missing api/local-model.openapi.yaml');
const result={ok:problems.length===0,packages:available.length,configured:components.length,skipped,problems};
if(process.argv.includes('--json'))console.log(JSON.stringify(result,null,2));
else{
  for(const item of skipped)console.log(`Workspace check: skipping ${item.id}: ${item.reason}`);
  if(result.ok)console.log(`Workspace check passed: ${available.length} available components (${skipped.length} skipped).`);
  else for(const problem of problems)console.error(`Workspace check: ${problem}`);
}
if(!result.ok)process.exitCode=1;

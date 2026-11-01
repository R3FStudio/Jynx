import {readFile,stat} from 'node:fs/promises';
import path from 'node:path';


export async function inspectComponents(root, {exclude = []} = {}) {
  const config=JSON.parse(await readFile(path.join(root,'workspace.config.json'),'utf8'));
  const rootPackage=JSON.parse(await readFile(path.join(root,'package.json'),'utf8'));
  if(!Array.isArray(config.buildOrder)||!config.packages)throw new Error('Invalid workspace component configuration.');
  const components=[];const seen=new Set();
  for(const id of config.buildOrder){
    const entry=config.packages[id];
    const component={id,...entry,status:'ready',reason:'',directory:'',manifest:null};
    components.push(component);
    if(seen.has(id)||!entry||typeof entry.path!=='string'||typeof entry.name!=='string'){
      component.status='invalid';component.reason=`Invalid or duplicate component configuration: ${id}`;continue;
    }
    seen.add(id);
    const directory=path.resolve(root,entry.path);const relative=path.relative(root,directory);
    if(!relative||relative.startsWith('..')||path.isAbsolute(relative)){
      component.status='invalid';component.reason='Component path must be inside the workspace.';continue;
    }
    component.directory=directory;
    if(exclude.includes(id)){component.status="excluded";component.reason="Excluded by build selection";continue;}
    try{
      const info=await stat(directory);
      if(!info.isDirectory())throw new Error('Component path is not a directory.');
      component.manifest=JSON.parse(await readFile(path.join(directory,'package.json'),'utf8'));
    }catch(error){
      component.status=error.code==='ENOENT'?'missing':'invalid';
      component.reason=error.code==='ENOENT'?`${entry.path}/package.json is not present`:error.message;
      continue;
    }
    if(component.manifest.name!==entry.name){component.status='invalid';component.reason=`Expected package name ${entry.name}, found ${component.manifest.name}`;}
    else if(!component.manifest.scripts?.build){component.status='invalid';component.reason='Present component has no build script.';}
    else if(!entry.standalone && !rootPackage.workspaces?.includes(entry.path)){component.status='invalid';component.reason='Present component is missing from root npm workspaces.';}
  }
  const byId=new Map(components.map(component=>[component.id,component]));


  for(const component of components){
    if(component.status!=='ready')continue;
    const dependencies=component.sourceDependencies??[];
    if(!Array.isArray(dependencies)||dependencies.some(id=>typeof id!=='string'||!byId.has(id))){
      component.status='invalid';component.reason='Invalid sourceDependencies configuration.';
    }
  }
  let changed=true;
  while(changed){
    changed=false;
    for(const component of components){
      if(component.status!=='ready')continue;
      const missing=(component.sourceDependencies??[]).find(id=>byId.get(id).status!=='ready');
      if(missing){component.status='unavailable';component.reason=`Requires source component ${missing}, which is ${byId.get(missing).status}`;changed=true;}
    }
  }
  return {config,rootPackage,components};
}

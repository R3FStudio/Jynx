import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
export function githubRepository(remote) {
  const match=/^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+?)(?:\.git)?\/?$/.exec(remote);
  if(!match)throw new Error('The remote must be a github.com repository URL.');
  return match[1];
}
export async function repositoryMetadata(repository,token,fetcher=fetch) {
  const response=await fetcher(`https://api.github.com/repos/${repository}`,{headers:{Accept:'application/vnd.github+json',...(token?{Authorization:`Bearer ${token}`}:{})},signal:AbortSignal.timeout(15000)});
  if(!response.ok)throw new Error(`Cannot verify ${repository} visibility (HTTP ${response.status}). Authenticate with repository access before pushing.`);
  return response.json();
}
export async function requirePrivateRemote(remote,token,fetcher=fetch) {
  const repository=githubRepository(remote);const metadata=await repositoryMetadata(repository,token,fetcher);
  if(metadata.private!==true)throw new Error('Push blocked: this linked repository requires a private GitHub destination.');
  return repository;
}
export function githubCredential(repository) {
  if(process.env.JYNX_PRIVATE_SOURCE_TOKEN)return process.env.JYNX_PRIVATE_SOURCE_TOKEN;
  if(process.env.GH_TOKEN||process.env.GITHUB_TOKEN)return process.env.GH_TOKEN||process.env.GITHUB_TOKEN;
  try{return execFileSync('gh',['auth','token'],{encoding:'utf8',stdio:['ignore','pipe','ignore']}).trim();}catch{}
  try{
    const response=execFileSync('git',['credential','fill'],{input:`protocol=https\nhost=github.com\npath=${repository}.git\n\n`,encoding:'utf8',env:{...process.env,GIT_TERMINAL_PROMPT:'0',GCM_INTERACTIVE:'Never'},stdio:['pipe','pipe','ignore']});
    return response.split(/\r?\n/).find(line=>line.startsWith('password='))?.slice(9);
  }catch{return undefined;}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
  try{
    const remote=process.argv[3]||process.argv[2];if(!remote)throw new Error('Missing push destination.');
    const repository=githubRepository(remote);await requirePrivateRemote(remote,githubCredential(repository));
  }catch(error){console.error(error.message);process.exitCode=1;}
}

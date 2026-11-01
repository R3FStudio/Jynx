import assert from 'node:assert/strict';import test from 'node:test';import {githubRepository,requirePrivateRemote} from '../private-remote.mjs';
test('linked repository pushes require verified GitHub privacy, never a folder name',async()=>{
  assert.equal(githubRepository('https://github.com/Example/Jynx-Desktop.git'),'Example/Jynx-Desktop');
  assert.equal(githubRepository('git@github.com:Example/Jynx-Desktop.git'),'Example/Jynx-Desktop');
  await assert.rejects(requirePrivateRemote('https://github.com/Example/public.git','test',async()=>({ok:true,json:async()=>({private:false})})),/Push blocked/);
  await assert.rejects(requirePrivateRemote('https://github.com/Example/private.git','test',async()=>({ok:false,status:404})),/Cannot verify/);
  assert.equal(await requirePrivateRemote('https://github.com/Example/private.git','test',async()=>({ok:true,json:async()=>({private:true})})),'Example/private');
  assert.throws(()=>githubRepository('https://github.com.evil.example/Example/private.git'),/github.com/);
});

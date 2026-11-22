import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

SERVER = Path(__file__).resolve().parents[1] / 'server.py'
spec = importlib.util.spec_from_file_location('jynx_computer', SERVER)
link = importlib.util.module_from_spec(spec)
spec.loader.exec_module(link)

class Editable:
    def __init__(self): self.value = None
    def set_text_contents(self, value): self.value = value; return True
class Node:
    def __init__(self): self.editable = Editable()
    def get_editable_text_iface(self): return self.editable

class LinkTests(unittest.TestCase):
    def tearDown(self): link.TARGETS.clear()
    def test_protocol_discovery_does_not_capture_or_read_apps(self):
        requests = [{'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2024-11-05'}},
                    {'jsonrpc':'2.0','method':'notifications/initialized'},
                    {'jsonrpc':'2.0','id':2,'method':'tools/list'}]
        result = subprocess.run([sys.executable, str(SERVER)],input='\n'.join(map(json.dumps,requests))+'\n',capture_output=True,text=True,check=True)
        responses = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(responses),2)
        self.assertEqual(responses[0]['result']['serverInfo']['name'],'jynx-computer-use')
        self.assertEqual({tool['name'] for tool in responses[1]['result']['tools']},{'computer_capabilities','computer_snapshot','computer_action','computer_screenshot'})
    def test_live_native_text_action_uses_element_interface(self):
        node = Node();link.TARGETS['synthetic'] = node
        with patch.object(link.platform,'system',return_value='Linux'):
            result=link.action({'target':'synthetic','action':'set_text','text':'Literal $(text) "and" \nUnicode café'})
        self.assertTrue(result['accepted'])
        self.assertEqual(node.editable.value,'Literal $(text) "and" \nUnicode café')
    def test_expired_targets_and_unknown_actions_fail_honestly(self):
        result=link.respond({'method':'tools/call','params':{'name':'computer_action','arguments':{'target':'expired','action':'invoke'}}})
        self.assertTrue(result['isError'])
        self.assertIn('fresh computer_snapshot',result['content'][0]['text'])
        link.TARGETS['synthetic']=Node()
        with self.assertRaises(ValueError): link.action({'target':'synthetic','action':'arbitrary_script'})
        with self.assertRaises(ValueError): link.action({'target':'synthetic','action':'set_text','text':'x'*10001})
    def test_actions_never_claim_success_after_native_refusal(self):
        node=Node();node.editable.set_text_contents=lambda text:False;link.TARGETS['synthetic']=node
        with patch.object(link.platform,'system',return_value='Linux'):
            with self.assertRaisesRegex(RuntimeError,'did not accept'):link.action({'target':'synthetic','action':'set_text','text':'fixture'})

if __name__=='__main__': unittest.main()

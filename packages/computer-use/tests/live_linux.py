import importlib.util, json, subprocess, sys, tempfile, time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
fixture='''import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,GLib
GLib.set_prgname('jynx-native-link-fixture')
GLib.set_application_name('Jynx native linker fixture')
window=Gtk.Window(title='Jynx native linker fixture')
box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=16)
window.add(box)
entry=Gtk.Entry();entry.get_accessible().set_name('Synthetic entry')
button=Gtk.Button(label='Run synthetic check')
label=Gtk.Label(label='Waiting for native action')
button.connect('clicked',lambda *_:label.set_text('Verified native action'))
for widget in [entry,button,label]:box.pack_start(widget,True,True,0)
window.set_default_size(420,180);window.connect('destroy',Gtk.main_quit);window.show_all();Gtk.main()
'''
process=subprocess.Popen([sys.executable,'-c',fixture],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
try:
 spec=importlib.util.spec_from_file_location('link',root/'server.py');link=importlib.util.module_from_spec(spec);spec.loader.exec_module(link)
 deadline=time.monotonic()+15;nodes=[]
 while time.monotonic()<deadline:
  report=link.snapshot({'application':'jynx-native-link-fixture'});nodes=report['nodes']
  if any(n.get('name')=='Synthetic entry' for n in nodes):break
  time.sleep(.2)
 else:
  print('Synthetic process status:',process.poll(), 'Warnings:',report.get('warnings'))
  if process.poll() is not None:print(process.stderr.read().decode())
  raise RuntimeError('Synthetic GTK app was not discoverable: '+json.dumps(nodes))
 entry=next(n for n in nodes if n.get('name')=='Synthetic entry')
 assert link.action({'target':entry['id'],'action':'set_text','text':'Verified native text'})['accepted']
 report=link.snapshot({'application':'jynx-native-link-fixture'});nodes=report['nodes']
 assert any(n.get('text')=='Verified native text' for n in nodes),json.dumps(nodes)
 button=next(n for n in nodes if n.get('name')=='Run synthetic check' and n.get('actions'))
 assert link.action({'target':button['id'],'action':'invoke'})['accepted']
 deadline=time.monotonic()+5
 while time.monotonic()<deadline:
  report=link.snapshot({'application':'jynx-native-link-fixture'});nodes=report['nodes']
  if any(n.get('name')=='Verified native action' or n.get('text')=='Verified native action' for n in nodes):break
  time.sleep(.1)
 else:raise RuntimeError('Native invoke was accepted but its resulting state was not observable')
 print('PASS: live Linux AT-SPI discovers the synthetic app, enters text, invokes its button and verifies the resulting state without screenshots')
finally:
 process.terminate()
 try:process.wait(timeout=5)
 except subprocess.TimeoutExpired:process.kill();process.wait()

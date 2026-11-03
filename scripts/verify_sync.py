"""Local fault checks for isolated workers, immutable input leases, IDs and hash-bound captures."""
import argparse,json,os,pathlib,subprocess,tempfile,time,signal,psutil
p=argparse.ArgumentParser();p.add_argument('exe');a=p.parse_args();exe=str(pathlib.Path(a.exe).resolve())
with tempfile.TemporaryDirectory(prefix='framelet-sync-') as tmp, tempfile.TemporaryDirectory(prefix='framelet-readonly-') as ro:
 root=pathlib.Path(tmp);env={**os.environ,'FRAMELET_MCP_ROOT':tmp,'FRAMELET_MCP_READ_ROOTS':json.dumps([ro]),'LOCALAPPDATA':tmp}
 host=subprocess.Popen([exe,'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',env=env);seq=0
 def rpc(method,params=None):
  global seq;seq+=1;host.stdin.write(json.dumps({'jsonrpc':'2.0','id':seq,'method':method,'params':params or {}})+'\n');host.stdin.flush();return json.loads(host.stdout.readline())['result']
 def tool(name,args=None,ok=True):
  r=rpc('tools/call',{'name':name,'arguments':args or {}});assert (not r.get('isError',False))==ok,r;return r.get('structuredContent',{}).get('result')
 def terminal(sid):
  deadline=time.monotonic()+20
  while time.monotonic()<deadline:
   s=tool('framelet_recording_status',{'session_id':sid})
   if s['terminal']:return s
   time.sleep(.03)
  raise AssertionError('terminal timeout')
 try:
  common={'region':{'x':0,'y':0,'width':64,'height':64},'fps':5}
  sid=tool('framelet_start_recording',{**common,'output':'killed.gif','duration':30})['session_id']
  deadline=time.monotonic()+10
  while tool('framelet_recording_status',{'session_id':sid}).get('state')!='recording':assert time.monotonic()<deadline;time.sleep(.02)
  children=[c for c in psutil.Process(host.pid).children() if 'worker-record' in c.cmdline()];assert len(children)==1,children;children[0].kill();children[0].wait(timeout=5)
  failed=terminal(sid);assert failed['result']['state']=='failed' and failed['result']['failure'];assert rpc('ping')=={}
  r=subprocess.run([exe,'recover',str(root/'killed.gif')],capture_output=True,text=True,encoding='utf-8',env=env,timeout=10);assert r.returncode==0,r.stderr;assert json.loads(r.stdout)['result']['state']=='incomplete'
  assert tool('framelet_recover',{'input':'killed.gif'})['state']=='incomplete'
  # A suspended native worker must not hang the host indefinitely.
  hang=tool('framelet_start_recording',{**common,'output':'hung.gif','duration':60})['session_id']
  deadline=time.monotonic()+10
  while tool('framelet_recording_status',{'session_id':hang}).get('state')!='recording':assert time.monotonic()<deadline;time.sleep(.02)
  child=[c for c in psutil.Process(host.pid).children() if 'worker-record' in c.cmdline()][0];child.suspend()
  tool('framelet_stop_recording',{'session_id':hang});hung=terminal(hang)
  assert hung['result']['state']=='failed' and 'timeout' in hung['result']['error'],hung
  assert rpc('ping')=={}
  sid=tool('framelet_start_recording',{**common,'output':'good.gif','duration':.8})['session_id'];done=terminal(sid);assert done['result']['state']=='completed',done
  gif=root/'good.gif';side=gif.with_suffix('.timeline.json');manifest=gif.with_suffix('.capture.json');d=json.loads(side.read_text());assert d['schema']==3 and manifest.exists()
  ids=[s['id'] for s in d['samples']]+[e['id'] for e in d['events']];assert all(i is not None for i in ids) and len(ids)==len(set(ids))
  tool('framelet_timeline',{'input':'good.gif','at':.1})
  import shutil
  for suffix in ['.gif','.timeline.json','.capture.json']:shutil.copy2(gif.with_suffix(suffix),pathlib.Path(ro)/('good'+suffix))
  tool('framelet_timeline',{'input':str(pathlib.Path(ro)/'good.gif'),'at':.1})
  tool('framelet_recover',{'input':str(pathlib.Path(ro)/'good.gif')},ok=False)

  original=gif.read_bytes();mut=bytearray(original);mut[-2]^=1;gif.write_bytes(mut);tool('framelet_timeline',{'input':'good.gif'},ok=False);gif.write_bytes(original)
  # Recover a complete pair with its commit marker missing (simulated interruption after both files).
  manifest.unlink();marker=gif.with_suffix('.pending.json');marker.write_text(json.dumps({'schema':1,'gif':'good.gif','pid':0,'owner_created':0,'transaction_id':'test'}));r=tool('framelet_recover',{'input':'good.gif'});assert r['state']=='recovered' and manifest.exists()
  assert rpc('ping')=={}
  print('PASS worker forced exit is terminal failure; MCP survives; next recording succeeds; incomplete recovery does not invent data')
  print('PASS schema-3 unique IDs, SHA-256 tamper detection, commit manifest and interrupted-pair recovery')
 finally:
  host.stdin.close();host.wait(timeout=15);assert host.returncode==0,host.stderr.read()
print(json.dumps({'ok':True}))

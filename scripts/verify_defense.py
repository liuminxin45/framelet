"""Failure-injection checks: malformed sidecars, protocol recovery and diagnostic I/O."""
import hashlib,uuid
import argparse,copy,json,os,pathlib,subprocess,tempfile
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('exe');a=p.parse_args();exe=str(pathlib.Path(a.exe).resolve())
with tempfile.TemporaryDirectory(prefix='framelet-defense-') as tmp:
 root=pathlib.Path(tmp);env={**os.environ,'LOCALAPPDATA':str(root),'FRAMELET_MCP_ROOT':str(root),'FRAMELET_MCP_READ_ROOTS':'[]'}
 def run(*args,ok=True,environ=env):
  r=subprocess.run([exe,*map(str,args)],capture_output=True,text=True,encoding='utf-8',env=environ,timeout=30)
  assert (r.returncode==0)==ok,(args,r.stdout,r.stderr)
  assert 'panicked at' not in r.stderr
  return json.loads(r.stdout if ok else r.stderr)
 raw=root/'source.gif';im=Image.new('RGB',(8,8),'red');im.save(raw,save_all=True,append_images=[Image.new('RGB',(8,8),'blue')],duration=500,loop=0)
 info=run('inspect',raw)['result'];side=raw.with_suffix('.timeline.json')
 s=dict(seq=0,utc_us=100000000,capture_us=0,capture_end_us=100,coverage_start_us=0,hold_end_us=500000,media_start_us=0,media_end_us=500000,gif_frame=0,cursor=[2,2]);s2={**s,'seq':1,'utc_us':100500000,'capture_us':500000,'capture_end_us':500100,'coverage_start_us':500000,'hold_end_us':1000000,'media_start_us':500000,'media_end_us':1000000,'gif_frame':1}
 e=dict(seq=0,utc_us=100100000,mono_us=100000,kind=513,point=[2,2],injected=True,wheel=0,media_us=100000,gif_frame=0)
 data=dict(schema=2,asset_id=uuid.uuid4().hex,gif_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),session_id=uuid.uuid4().hex,region=dict(x=0,y=0,width=8,height=8),width=8,height=8,gif_bytes=raw.stat().st_size,duration_ms=1000,frames=2,samples=[s,s2],events=[e],dropped_events=0,warnings=[])
 for i,sample in enumerate(data['samples']):sample['id']=i+1
 for i,event in enumerate(data['events']):
  sample=next(s for s in data['samples'] if s['media_start_us']<=event['media_us']<s['media_end_us'])
  event.update(id=len(data['samples'])+i+1,device='mouse',vk=None,scan=None,flags=0,key_name=None,sample_id=sample['id'],offset_us=event['mono_us']-sample['capture_us'])
 def store(d):side.write_text(json.dumps(d),encoding='utf-8')
 store(data);run('timeline',raw)
 cases=[]
 for key in ['schema','missing_device','missing_coverage','unknown_field','missing_asset']:
  d=copy.deepcopy(data)
  if key=='schema':d['schema']=1
  elif key=='missing_device':del d['events'][0]['device']
  elif key=='missing_coverage':del d['samples'][0]['coverage_start_us']
  elif key=='unknown_field':d['legacy_option']=True
  else:del d['asset_id']
  cases.append(d)
 for stream,key,value in [('events','sample_id',2),('events','offset_us',100001),('events','sample_id',None),('samples','gif_frame',1),('samples','cursor',[-1,2]),('samples','hold_end_us',0),('samples','media_end_us',750000),('events','gif_frame',1),('events','gif_frame',None),('events','point',[8,2])]:
  d=copy.deepcopy(data);d[stream][0][key]=value;cases.append(d)
 d=copy.deepcopy(data);d['samples'].reverse();cases.append(d)
 for d in cases:
  store(d);run('timeline',raw,ok=False);out=root/'must-not-publish.gif';run('edit',raw,'--output',out,ok=False);assert not out.exists()
 store(data)
 q=run('timeline',raw,'--utc-us',999999999)['result'];assert q['sample'] is None and q['matched_media_us'] is None and q['events']==[]
 run('record','--fps','nope',ok=False)
 print('PASS hostile frame/point/range/order metadata rejected before publication; out-of-range query has no false match')
 requests=[{'jsonrpc':'2.0','id':i+1,'method':'tools/call','params':{'name':'framelet_timeline','arguments':{'input':'source.gif','utc_us':'bad'}}} for i in range(3)]
 requests.append({'jsonrpc':'2.0','id':99,'method':'ping'})
 r=subprocess.run([exe,'mcp'],input='\n'.join(map(json.dumps,requests))+'\n',capture_output=True,text=True,encoding='utf-8',env=env,timeout=30);assert r.returncode==0,r.stderr
 replies=list(map(json.loads,r.stdout.splitlines()));assert replies[-1]['result']=={}
 assert all(x['result']['isError'] and x['result']['structuredContent']['failure']['stage']=='mcp.tool' for x in replies[:-1])
 # Small deterministic mutation corpus, not a substitute for a sustained fuzzer.
 original=raw.read_bytes();requests=[]
 for i in range(min(64,len(original))):
  mutated=bytearray(original);mutated[i]^=255;name=f'mutation-{i}.gif';(root/name).write_bytes(mutated)
  requests.append({'jsonrpc':'2.0','id':i,'method':'tools/call','params':{'name':'framelet_inspect','arguments':{'input':name}}})
 requests.append({'jsonrpc':'2.0','id':999,'method':'ping'})
 r=subprocess.run([exe,'mcp'],input='\n'.join(map(json.dumps,requests))+'\n',capture_output=True,text=True,encoding='utf-8',env=env,timeout=30)
 assert r.returncode==0,r.stderr
 replies=list(map(json.loads,r.stdout.splitlines()));assert len(replies)==len(requests) and replies[-1]['result']=={}
 print('PASS 64-byte GIF mutation corpus retains server responsiveness')
 logs=root/'Framelet/Logs';records=[json.loads(x) for p in logs.glob('failures-*.jsonl') for x in p.read_text(encoding='utf-8').splitlines()];assert all(x['code'] and x['stage'] for x in records)
 child=subprocess.Popen([exe,'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',env=env)
 log=logs/f'failures-{child.pid}.jsonl';log.write_text(' '*1048577,encoding='utf-8')
 message={'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':'framelet_inspect','arguments':{'input':'missing.gif'}}}
 stdout,stderr=child.communicate(json.dumps(message)+'\n',timeout=30);assert child.returncode==0,stderr
 assert (logs/f'failures-{child.pid}.previous.jsonl').exists();json.loads(log.read_text(encoding='utf-8').strip())
 blocked=root/'blocked';(blocked/'Framelet').mkdir(parents=True);(blocked/'Framelet/Logs').write_text('not-a-directory');failure=run('inspect',root/'missing.gif',ok=False,environ={**env,'LOCALAPPDATA':str(blocked)})['failure'];assert failure['log_error']
 assert not list(root.glob('*.partial'))
 print('PASS structured tool failures preserve ping; rotating diagnostics; diagnostic write failure remains a structured error')
print(json.dumps({'ok':True,'malformed_timeline_cases':len(cases)}))

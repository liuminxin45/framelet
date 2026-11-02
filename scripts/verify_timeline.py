"""Independent timing/sidecar regression checks; Python/Pillow are dev-only."""
import hashlib,uuid
import argparse,json,pathlib,subprocess,tempfile,time
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--desktop',action='store_true');a=p.parse_args();exe=str(pathlib.Path(a.exe).resolve())
def run(*args,ok=True):
 r=subprocess.run([exe,*map(str,args)],capture_output=True,text=True,encoding='utf-8',timeout=45)
 assert (r.returncode==0)==ok,(args,r.stdout,r.stderr)
 return json.loads(r.stdout if ok else r.stderr)
def side(p):return p.with_suffix('.timeline.json')
with tempfile.TemporaryDirectory(prefix='framelet-timeline-') as tmp:
 root=pathlib.Path(tmp);raw=root/'raw.gif'
 ims=[Image.new('RGB',(16,16),c) for c in ['red','blue','green','white']]
 ims[0].save(raw,save_all=True,append_images=ims[1:],duration=[100,300,100,500],loop=0)
 info=run('inspect',raw)['result'];clock=1793595000000000
 samples=[];at=0
 for i,d in enumerate([100000,300000,100000,500000]):
  samples.append(dict(seq=i,utc_us=clock+at,capture_us=at,capture_end_us=at+100,coverage_start_us=at,hold_end_us=at+d,media_start_us=at,media_end_us=at+d,gif_frame=i,cursor=[5,5]));at+=d
 events=[dict(seq=i,utc_us=clock+t,mono_us=t,kind=513,point=[5,5],injected=True,wheel=0,media_us=t,gif_frame=next(s['gif_frame'] for s in samples if s['media_start_us']<=t<s['media_end_us'])) for i,t in enumerate([125000,450000,900000])]
 data=dict(schema=2,asset_id=uuid.uuid4().hex,gif_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),session_id=uuid.uuid4().hex,region=dict(x=-20,y=30,width=32,height=32),width=16,height=16,gif_bytes=raw.stat().st_size,duration_ms=info['duration_ms'],frames=info['frames'],samples=samples,events=events,dropped_events=0,warnings=[])
 for i,sample in enumerate(data['samples']):sample['id']=i+1
 for i,event in enumerate(data['events']):
  sample=next(s for s in data['samples'] if s['media_start_us']<=event['media_us']<s['media_end_us'])
  event.update(id=len(data['samples'])+i+1,device='mouse',vk=None,scan=None,flags=0,key_name=None,sample_id=sample['id'],offset_us=event['mono_us']-sample['capture_us'])
 side(raw).write_text(json.dumps(data),encoding='utf-8')
 q=run('timeline',raw,'--utc-us',clock+450000)['result'];assert q['sample']['seq']==2 and q['matched_media_us']==450000
 for speed in [0.01,0.5,1,2,100]:
  out=root/f'edit-{speed}.gif';run('edit',raw,'--start',.12,'--end',.93,'--speed',speed,'--output',out)
  new=json.loads(side(out).read_text(encoding='utf-8'));q=run('timeline',out)['result'];assert len(new['events'])==3
  assert [e['utc_us'] for e in new['events']]==[e['utc_us'] for e in events]
  assert all(s['utc_us']==samples[s['seq']]['utc_us'] for s in new['samples'])
  im=Image.open(out);bounds=[];pos=0
  for i in range(im.n_frames):im.seek(i);delay=im.info['duration']*1000;bounds.append((pos,pos+delay));pos+=delay
  im.close()
  for e in new['events']:assert bounds[e['gif_frame']][0]<=e['media_us']<bounds[e['gif_frame']][1]
  twice=root/f'twice-{speed}.gif';run('edit',out,'--speed',2,'--output',twice);run('timeline',twice)
 print('PASS UTC lookup, non-aligned trims, repeated speed edits and independent GIF interval checks')
 broken=root/'broken.gif';broken.write_bytes(raw.read_bytes());bad=dict(data);bad['gif_bytes']+=1;side(broken).write_text(json.dumps(bad));target=root/'refused.gif';run('edit',broken,'--output',target,ok=False);assert not target.exists()
 run('timeline',raw,'--at',.2,'--utc-us',clock,ok=False)
 absent=root/'absent.gif';absent.write_bytes(raw.read_bytes());run('timeline',absent,ok=False)
 print('PASS mismatch/ambiguous queries reject safely; old GIFs do not invent metadata')
 if a.desktop:
  live=root/'live.gif';result=run('record','--region','0,0,64,64','--mask','0,0,64,64','--fps',5,'--duration',1.2,'--output',live)['result']
  live_data=json.loads(side(live).read_text(encoding='utf-8'));assert result['timeline'] and len(live_data['samples'])>=3 and live_data['frames']==1
  assert all(s['cursor'] is None for s in live_data['samples']) and all(e['point'] is None for e in live_data['events'])
  run('timeline',live,'--at',.3)
  print('PASS live default timestamps, merged static samples and privacy-mask coordinates')
  race=root/'race.gif'
  child=subprocess.Popen([exe,'record','--region','0,0,16,16','--fps','5','--duration','1','--output',str(race)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8')
  deadline=time.monotonic()+10
  while not list(root.glob('race.gif.*.partial')):
   assert child.poll() is None and time.monotonic()<deadline;time.sleep(.01)
  side(race).write_text('preserve-me',encoding='utf-8')
  stdout,stderr=child.communicate(timeout=15);assert child.returncode==0,stderr
  result=json.loads(stdout)['result'];assert result['timeline'] is None and result['warnings'] and race.exists()
  assert side(race).read_text()=='preserve-me';run('inspect',race)
  print('PASS racing metadata collision preserves GIF, existing sidecar and explicit warning')

 assert not list(root.glob('*.partial'))
print(json.dumps({'ok':True,'desktop':a.desktop}))

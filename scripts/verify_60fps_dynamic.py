"""Inspect a real recording of scripts/fixtures/60fps.html; never inject GUI input or alter images."""
import argparse,hashlib,json,pathlib,subprocess
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('gif');p.add_argument('--output',required=True);a=p.parse_args()
exe=pathlib.Path(a.exe).resolve();gif=pathlib.Path(a.gif).resolve()
r=subprocess.run([str(exe),'timeline',str(gif),'--at','1'],capture_output=True,text=True,encoding='utf-8',timeout=30);assert r.returncode==0,r.stderr
d=json.loads(gif.with_suffix('.timeline.json').read_text(encoding='utf-8'))
delays=[];unique=set();matched=0
with Image.open(gif) as im:
 for i in range(im.n_frames):
  im.seek(i);rgb=im.convert('RGB');delays.append(im.info['duration']);unique.add(hashlib.sha256(rgb.tobytes()).hexdigest())
  red,green,blue=rgb.getpixel((0,0))
  matched+=((red*pow(17,-1,256))%256==(green*pow(29,-1,256))%256==(blue*pow(43,-1,256))%256)
assert sum(delays)==d['duration_ms']==5000
assert len(delays)==d['frames'] and len(unique)>=len(delays)*.9
assert matched/len(delays)>=.98,'capture may include another surface; do not publish this sample'
fps=len(d['samples'])/(d['duration_ms']/1000);assert fps>=54
source_span=d['samples'][-1]['capture_us']-d['samples'][0]['capture_us']
proof={'ok':True,'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'requested_fps':60,'scenario':'Real Edge page, scripts/fixtures/60fps.html, source observed above 60 updates/s; capture region 100,350,256,128; 5 seconds.',
 'samples':len(d['samples']),'actual_sample_fps':fps,'actual_sample_interval_fps':(len(d['samples'])-1)*1e6/source_span,'gif_frames':len(delays),'distinct_decoded_images':len(unique),'duration_ms':sum(delays),'short_delay_frames':sum(t<=10 for t in delays),'fixture_pixel_match_ratio':matched/len(delays),
 'chromium_policy_projected_duration_ms':sum(100 if t<=10 else t for t in delays),'projection_note':'Computed from current Chromium source policy; not a measurement of wall-clock playback.','gif_sha256':hashlib.sha256(gif.read_bytes()).hexdigest()}
pathlib.Path(a.output).write_text(json.dumps(proof,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(proof,ensure_ascii=False))

"""Real CLI/MCP capture at 60 FPS. Pillow validates files independently; rate gating is opt-in."""
import argparse,hashlib,json,pathlib,subprocess,time
from PIL import Image
from verify_series import MCP,run,validate
p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',required=True);p.add_argument('--require-rate',action='store_true');a=p.parse_args()
root=pathlib.Path(a.output).resolve();root.mkdir(parents=True,exist_ok=False);exe=str(pathlib.Path(a.exe).resolve())
target=root/'cli-60.gif';began=time.monotonic()
stats=run(exe,'record','--output',target,'--region','0,0,64,64','--mask','0,0,64,64','--fps','60','--duration','3')
elapsed=time.monotonic()-began
assert abs(stats['duration_ms']-3000)<=10,stats
rate=stats['samples']/(stats['duration_ms']/1000)
assert stats['playback']['requested_capture_fps']==60
if a.require_rate:assert rate>=54,(rate,stats)
with Image.open(target) as im:
 total=0
 for i in range(im.n_frames):im.seek(i);total+=im.info['duration'];im.convert('RGB').load()
 assert total==stats['duration_ms']
run(exe,'timeline',target,'--at','1')
bad=subprocess.run([exe,'record','--output',str(root/'bad.gif'),'--region','0,0,64,64','--fps','61'],capture_output=True,text=True,encoding='utf-8',timeout=10)
assert bad.returncode!=0 and not (root/'bad.gif').exists()
m=MCP(exe,root)
try:
 ctx=m.tool('framelet_context');assert ctx['capture_guidance']['fps_range']=={'min':1,'max':60}
 s=m.tool('framelet_start_series',{'output_dir':str(root),'name':'mcp-60','duration':3.2,'segment_seconds':1,'fps':60,'region':{'x':0,'y':0,'width':64,'height':64},'masks':[{'x':0,'y':0,'width':64,'height':64}]})
 done=m.wait(s['session_id'],lambda s:s['terminal'])
 assert done['result']['state']=='completed',done
 index=validate(exe,s['directory'],[1000,1000,1000,200])
 for part in index['parts']:assert part['recording']['playback']['requested_capture_fps']==60
 m.tool('framelet_frame_at',{'input':s['directory'],'at':1.2,'output':str(root/'mcp-probe.png')})
finally:m.close()
proof={'ok':True,'exe_sha256':hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest(),'scenario':'Real GDI capture, masked 64x64 region; sampling-rate test, not unique display refresh verification.','cli':stats,'cli_elapsed_seconds':elapsed,'actual_sample_fps':rate,'mcp_parts_ms':[x['recording']['duration_ms'] for x in index['parts']]}
(root/'verification.json').write_text(json.dumps(proof,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'ok':True,'actual_sample_fps':rate,'samples':stats['samples'],'gif_ms':stats['duration_ms'],'mcp_parts_ms':proof['mcp_parts_ms']}))

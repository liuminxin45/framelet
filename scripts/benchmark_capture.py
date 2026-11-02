"""Measure masked local capture; benchmark output is workload-specific, not a general claim."""
import argparse,json,os,pathlib,subprocess,tempfile,time,psutil
p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',type=pathlib.Path,default=pathlib.Path('work/capture-benchmark.json'));a=p.parse_args();exe=str(pathlib.Path(a.exe).resolve());results=[]
r=subprocess.run([exe,'doctor'],capture_output=True,text=True,encoding='utf-8');doctor=json.loads(r.stdout);desktop=doctor['desktop'];w=min(960,desktop['width']);h=min(540,desktop['height'])
with tempfile.TemporaryDirectory(prefix='framelet-bench-') as tmp:
 for fps in [1,30]:
  target=pathlib.Path(tmp)/f'fps-{fps}.gif';region=f"{desktop['x']},{desktop['y']},{w},{h}";cmd=[exe,'record','--region',region,'--mask',f'0,0,{w},{h}','--fps',str(fps),'--duration','3','--output',str(target)]
  proc=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8');process=psutil.Process(proc.pid);peak=0;cpus={};began=time.monotonic()
  while proc.poll() is None:
   total=0
   try:members=[process,*process.children(recursive=True)]
   except psutil.Error:members=[]
   for member in members:
    try:
     total+=member.memory_info().rss;cpu=member.cpu_times();cpus[member.pid]=cpu.user+cpu.system
    except psutil.Error:pass
   peak=max(peak,total);assert time.monotonic()-began<40;time.sleep(.01)
  stdout,stderr=proc.communicate();assert proc.returncode==0,stderr;stats=json.loads(stdout)['result'];data=json.loads(target.with_suffix('.timeline.json').read_text(encoding='utf-8'))
  results.append({'requested_fps':fps,'samples':len(data['samples']),'recorded_ms':stats['duration_ms'],'observed_samples_per_second':round(len(data['samples'])/(stats['duration_ms']/1000),2),'host_plus_worker_peak_rss_bytes':peak,'observed_cpu_seconds':round(sum(cpus.values()),3),'elapsed_seconds':round(time.monotonic()-began,3)})
result={'ok':True,'scenario':'960x540 or desktop-limited GDI capture, fully masked static scene; includes host + worker; not a general benchmark','results':results};a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(result,indent=2));print(json.dumps(result))

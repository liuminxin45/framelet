"""Deterministic mutation campaign and resource measurement; development only."""
import argparse,io,json,os,pathlib,random,subprocess,tempfile,time,psutil,threading
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--iterations',type=int,default=1000);p.add_argument('--seed',type=int,default=42);p.add_argument('--output',type=pathlib.Path);a=p.parse_args();assert 1<=a.iterations<=10000
rng=random.Random(a.seed);exe=str(pathlib.Path(a.exe).resolve());started=time.monotonic()
with tempfile.TemporaryDirectory(prefix='framelet-fuzz-') as tmp:
 root=pathlib.Path(tmp);env={**os.environ,'FRAMELET_MCP_ROOT':tmp,'FRAMELET_MCP_READ_ROOTS':'[]','LOCALAPPDATA':tmp};proc=subprocess.Popen([exe,'mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,encoding='utf-8',env=env)
 gif=io.BytesIO();Image.new('RGB',(16,16),'red').save(gif,format='GIF');png=io.BytesIO();Image.new('RGB',(16,16),'blue').save(png,format='PNG');peak=[0];done=threading.Event()
 def monitor():
  process=psutil.Process(proc.pid)
  while not done.wait(.02):
   try:
    family=[process]+process.children(recursive=True)
    rss=0
    for member in family:
     try:rss+=member.memory_info().rss
     except psutil.NoSuchProcess:pass
    peak[0]=max(peak[0],rss)
    if time.monotonic()-started>120 or peak[0]>512*1024*1024:
     for member in reversed(family):
      try:member.kill()
      except psutil.NoSuchProcess:pass
     break
   except psutil.Error:break
 thread=threading.Thread(target=monitor,daemon=True);thread.start()
 try:
  for i in range(a.iterations):
   kind=i%3
   if kind<2:
    data=bytearray(gif.getvalue() if kind==0 else png.getvalue())
    for _ in range(rng.randint(1,5)):data[rng.randrange(len(data))]=rng.randrange(256)
    name='mutant.gif' if kind==0 else 'mutant.png';(root/name).write_bytes(data)
    request={'jsonrpc':'2.0','id':i,'method':'tools/call','params':{'name':'framelet_inspect' if kind==0 else 'framelet_read_image','arguments':{'input':name}}};line=json.dumps(request)
   else:
    line=rng.choice(['{','[]','null','{"jsonrpc":"bad","method":"ping"}',json.dumps({'jsonrpc':'2.0','id':i,'method':'tools/call','params':{'name':'bad','arguments':{'x':[None]*rng.randint(1,20)}}})])
   proc.stdin.write(line+'\n');proc.stdin.flush();reply=proc.stdout.readline();assert reply and isinstance(json.loads(reply),dict)
  proc.stdin.write(json.dumps({'jsonrpc':'2.0','id':99999,'method':'ping'})+'\n');proc.stdin.flush();assert json.loads(proc.stdout.readline())['result']=={}
  family=[psutil.Process(proc.pid)]+psutil.Process(proc.pid).children(recursive=True);cpu_seconds=0
  for member in family:
   try:
    cpu=member.cpu_times();cpu_seconds+=cpu.user+cpu.system
   except psutil.NoSuchProcess:pass
  result={'ok':True,'iterations':a.iterations,'seed':a.seed,'elapsed_seconds':round(time.monotonic()-started,3),'process_family_peak_rss_bytes':peak[0],'live_process_family_cpu_seconds':round(cpu_seconds,3),'note':'RSS sum includes shared pages; CPU is live family only; deterministic mutation campaign, not proof of absence of bugs or coverage-guided fuzzing'}
 finally:
  done.set();thread.join();proc.stdin.close();proc.wait(timeout=10)
 if a.output:a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(result,indent=2))
 print(json.dumps(result))

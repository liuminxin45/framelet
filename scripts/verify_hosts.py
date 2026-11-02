"""Exercise installed host implementations without LLM inference or user-profile writes."""
import argparse,json,os,queue,subprocess,threading,time,hashlib,shutil
from pathlib import Path
import psutil

def main():
    p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',required=True);p.add_argument('--codex',default='codex.cmd');p.add_argument('--claude');a=p.parse_args()
    a.codex=str(Path(a.codex).resolve()) if Path(a.codex).is_file() else shutil.which(a.codex)
    assert a.codex,'Codex executable not found'
    if a.claude:a.claude=str(Path(a.claude).resolve())
    exe=Path(a.exe).resolve();root=Path(a.output).resolve();root.mkdir(parents=True,exist_ok=False)
    env={**os.environ,'PYTHONUTF8':'1','LOCALAPPDATA':str(root/'local'),'APPDATA':str(root/'appdata'),'CODEX_HOME':str(root/'codex'),'CLAUDE_CONFIG_DIR':str(root/'claude')}
    config=root/'codex/config.toml';skill=root/'.agents/skills/framelet'
    def install(client,config,skill=None):
        args=[str(exe),'integrate','install','--client',client,'--config',str(config),'--output-root',str(root/'captures')]
        if skill:args+=['--skill-dir',str(skill)]
        v=subprocess.run(args,env=env,capture_output=True,text=True,encoding='utf-8',timeout=35);assert v.returncode==0,v.stderr
    install('codex',config,skill)
    log=(root/'codex.stderr.log').open('w',encoding='utf-8')
    host=subprocess.Popen([a.codex,'app-server','--stdio'],env=env,cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,encoding='utf-8')
    q=queue.Queue()
    def read():
        for line in host.stdout:
            try:q.put(json.loads(line))
            except json.JSONDecodeError:q.put({'invalid_line':line[:256]})
    threading.Thread(target=read,daemon=True).start();seq=0
    def call(method,params):
        nonlocal seq;seq+=1;host.stdin.write(json.dumps({'jsonrpc':'2.0','id':seq,'method':method,'params':params})+'\n');host.stdin.flush();end=time.monotonic()+50
        while True:
            item=q.get(timeout=max(.01,end-time.monotonic()))
            assert 'invalid_line' not in item,item
            if item.get('id')==seq:assert 'error' not in item,item;return item['result']
    try:
        initialized=call('initialize',{'clientInfo':{'name':'framelet-host-check','version':'1'},'capabilities':{'experimentalApi':True}})
        host.stdin.write('{"jsonrpc":"2.0","method":"initialized","params":{}}\n');host.stdin.flush()
        status=call('mcpServerStatus/list',{'serverName':'framelet','detail':'full'})
        servers=status.get('data',status.get('servers',[]));assert servers,status
        server=next(x for x in servers if x['name']=='framelet');tools=server.get('tools',{});assert len(tools)==23,server
        skills=call('skills/list',{'cwds':[str(root)],'forceReload':True})
        flat=[s for group in skills['data'] for s in group['skills']];assert any(Path(s['path']).resolve()==(skill/'SKILL.md').resolve() and s['enabled'] for s in flat), 'Installed skill not discovered'
        proof={'ok':True,'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'codex':{'host':initialized,'tools':len(tools),'skill_discovered':True,'application_protocol':'real Codex app-server; no model turn'},'claude':None}
    finally:
        try:children=psutil.Process(host.pid).children(recursive=True)
        except psutil.NoSuchProcess:children=[]
        host.stdin.close()
        try:host.wait(timeout=20)
        except subprocess.TimeoutExpired:
            for child in reversed(children):
                try:child.kill()
                except psutil.NoSuchProcess:pass
            host.kill();host.wait(timeout=5)
        log.close()
        _,alive=psutil.wait_procs(children,timeout=6);assert not alive,'host left child processes'
    if a.claude:
        profile=root/'claude';profile.mkdir(exist_ok=True);config=profile/'.claude.json';install('claude-code',config,profile/'skills/framelet')
        args=[a.claude,'mcp','list']
        v=subprocess.run(args,env=env,cwd=root,capture_output=True,text=True,encoding='utf-8',timeout=50)
        (root/'claude.log').write_text(v.stdout+'\n'+v.stderr,encoding='utf-8')
        assert v.returncode==0 and 'framelet' in v.stdout and ('Connected' in v.stdout or 'connected' in v.stdout),v.stdout+v.stderr
        version=subprocess.check_output([a.claude,'--version'],env=env,text=True,encoding='utf-8',timeout=15).strip()
        proof['claude']={'version':version,'connection':'real Claude Code mcp list reading installed user config','ok':True}
    (root/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
if __name__=='__main__':main()

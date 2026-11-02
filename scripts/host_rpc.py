"""Bounded Codex app-server client for host acceptance; never starts a model turn."""
import json,queue,subprocess,threading,time,os,shutil
from pathlib import Path
import psutil

def close_host(process, log):
    """Clean owned host helpers, but fail if any native/unknown child outlives EOF."""
    try:children=psutil.Process(process.pid).children(recursive=True)
    except psutil.NoSuchProcess:children=[]
    try:process.stdin.close()
    except BrokenPipeError:pass
    timed_out=False
    try:process.wait(timeout=20)
    except subprocess.TimeoutExpired:
        timed_out=True
        try:
            known={p.pid:p for p in children}
            known.update({p.pid:p for p in psutil.Process(process.pid).children(recursive=True)})
            children=list(known.values())
        except psutil.NoSuchProcess:pass
    _,alive=psutil.wait_procs(children,timeout=6)
    git=shutil.which('git')
    git_root=Path(git).resolve().parent.parent if git else None
    console=Path(os.environ['WINDIR'])/'System32/conhost.exe'
    helpers=[];unexpected=[]
    for child in reversed(alive):
        try:
            name=child.name().lower();exe=Path(child.exe()).resolve()
            allowed=(git_root is not None and exe.is_relative_to(git_root) and name in
                     ['git.exe','git-remote-https.exe','git-remote-http.exe','git-credential-manager.exe']) or exe==console
            (helpers if allowed else unexpected).append({'pid':child.pid,'name':name})
            child.kill()
        except psutil.NoSuchProcess:pass
    if process.poll() is None:process.kill();process.wait(timeout=5)
    _,remaining=psutil.wait_procs(children,timeout=5)
    log.write(json.dumps({'host_teardown':{'external_helpers_terminated':helpers,'unexpected_children':unexpected,'host_timed_out':timed_out}})+'\n')
    log.close()
    assert not remaining,'Cannot reap owned host children'
    assert not unexpected,f'Host left native or unknown children: {unexpected}'
    assert not timed_out,'Host did not exit within its EOF deadline'
    assert process.returncode==0,f'Host exited with {process.returncode}'
class Host:
    def __init__(self,command,env,cwd,log):
        self.log=log.open('w',encoding='utf-8');self.seq=0;self.q=queue.Queue()
        self.p=subprocess.Popen(command,env=env,cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log,text=True,encoding='utf-8')
        def read():
            for line in self.p.stdout:
                try:self.q.put(json.loads(line))
                except json.JSONDecodeError:self.q.put({'invalid_protocol':True})
            self.q.put(None)
        threading.Thread(target=read,daemon=True).start()
    def initialize(self):
        value=self.call('initialize',{'clientInfo':{'name':'framelet-package-check','version':'1'},'capabilities':{'experimentalApi':True}})
        self.p.stdin.write('{"jsonrpc":"2.0","method":"initialized","params":{}}\n');self.p.stdin.flush();return value
    def call(self,method,params,timeout=60):
        self.seq+=1;self.p.stdin.write(json.dumps({'jsonrpc':'2.0','id':self.seq,'method':method,'params':params})+'\n');self.p.stdin.flush();end=time.monotonic()+timeout
        while True:
            value=self.q.get(timeout=max(.01,end-time.monotonic()));assert value is not None,'Host exited before response';assert not value.get('invalid_protocol'),value
            if value.get('id')==self.seq:assert 'error' not in value,value;return value['result']
    def close(self):
        close_host(self.p,self.log)

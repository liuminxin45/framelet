"""Bounded Codex app-server client for host acceptance; never starts a model turn."""
import json,queue,subprocess,threading,time
import psutil
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
        try:children=psutil.Process(self.p.pid).children(recursive=True)
        except psutil.NoSuchProcess:children=[]
        self.p.stdin.close()
        try:self.p.wait(timeout=20)
        except subprocess.TimeoutExpired:
            for child in reversed(children):
                try:child.kill()
                except psutil.NoSuchProcess:pass
            self.p.kill();self.p.wait(timeout=5)
        self.log.close();_,alive=psutil.wait_procs(children,timeout=6);assert not alive,'Host left native children'

"""Real suspended children reproduce host-helper EOF races and strict native-orphan failure."""
import argparse,json,os,shutil,sys,time
from pathlib import Path
import psutil
from host_rpc import Host

CHILD_HOST = '''import subprocess,sys,psutil
from pathlib import Path
child=subprocess.Popen(sys.argv[2:],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,creationflags=subprocess.CREATE_NO_WINDOW)
psutil.Process(child.pid).suspend()
Path(sys.argv[1]).write_text(str(child.pid))
sys.stdin.read()
'''

def main():
    p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',required=True);a=p.parse_args()
    root=Path(a.output).resolve();root.mkdir(parents=True,exist_ok=False)
    proof={'ok':False,'checks':[]}
    for label,command,should_fail in [('git-helper',[shutil.which('git'),'hash-object','--stdin'],False),
                                       ('native-orphan',[str(Path(a.exe).resolve()),'mcp'],True)]:
        ready=root/(label+'.pid');log=root/(label+'.log')
        host=Host([sys.executable,'-c',CHILD_HOST,str(ready),*command],os.environ.copy(),root,log)
        failed=False
        try:
            end=time.monotonic()+10
            while not ready.exists():
                assert time.monotonic()<end and host.p.poll() is None,'fixture child startup failed'
                time.sleep(.01)
            child=psutil.Process(int(ready.read_text()))
        finally:
            try:host.close()
            except AssertionError as error:
                assert should_fail and 'native or unknown children' in str(error),str(error)
                failed=True
        assert failed==should_fail,label
        assert not child.is_running(),label
        proof['checks'].append(label)
    proof['ok']=True
    (root/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
if __name__=='__main__':main()

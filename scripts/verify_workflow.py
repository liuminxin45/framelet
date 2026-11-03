"""Real masked capture: CLI readiness and MCP save/review under host path redirection."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
import uuid

from verify_performance import Client, result
from run_checks import terminate_tree


def cli_ready(exe, root, series=False, early_stop=False):
    session = 'workflow-' + uuid.uuid4().hex
    output = root / (session + '.gif')
    command = [str(exe), 'record-series' if series else 'record', '--session', session,
               '--region', '0,0,64,64', '--mask', '0,0,64,64', '--fps', '5', '--events', '--duration', '60']
    command += ['--output-dir', str(root), '--name', session] if series else ['--output', str(output)]
    if early_stop:
        command += ['--delay', '5']
    child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8')
    events, incoming = [], queue.Queue()
    def read():
        for line in child.stderr:
            item = json.loads(line)
            events.append(item)
            incoming.put(item)
        incoming.put(None)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    try:
        if not early_stop:
            until = time.monotonic() + 10
            while True:
                event = incoming.get(timeout=max(.01, until-time.monotonic()))
                assert event is not None, events
                if event.get('event') == 'ready':
                    assert event['samples'] > 0 and child.poll() is None, event
                    break
        until = time.monotonic()+5
        while True:
            stop = subprocess.run([str(exe), 'stop', '--session', session], capture_output=True, timeout=10)
            if stop.returncode == 0:
                break
            assert early_stop and time.monotonic() < until, stop.stderr
            time.sleep(.03)
        child.wait(timeout=20)
        reader.join(timeout=3)
        assert not reader.is_alive()
        ready = [e for e in events if e.get('event')=='ready']
        if early_stop:
            assert child.returncode != 0 and not ready and not output.exists(), events
        else:
            value = json.loads(child.stdout.read())
            assert child.returncode==0 and value['ok'] and value['visual_review_required'], value
            assert len(ready)==1 and any(e.get('event')=='saved' for e in events), events
            saved = value['result']
            assert Path(saved['directory'] if series else saved['output']).exists(), saved
    finally:
        if child.poll() is None:
            terminate_tree(child)
        reader.join(timeout=3)


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('exe',type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    exe=args.exe.resolve(); root=args.output.resolve()
    root.mkdir(parents=True,exist_ok=False)
    proof=dict(ok=False,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),checks=[])
    cli_ready(exe,root)
    cli_ready(exe,root,series=True)
    cli_ready(exe,root,early_stop=True)
    proof['checks'].append('CLI single/series ready after samples, stop acknowledgement, final result and no false readiness before capture')
    # Use the real configured AppData location: directory and file canonical names
    # differ on a packaged host, unlike ordinary temporary-directory tests.
    capture_root=Path(os.environ['LOCALAPPDATA'])/'Framelet'/'Captures'
    capture_root.mkdir(parents=True,exist_ok=True)
    prefix='workflow-verification-'+uuid.uuid4().hex
    client=Client(exe,root,capture_root=capture_root)
    try:
        context=result(client.tool('framelet_context')[0])
        actual=Path(context['output_root'])
        name=prefix+'.gif'
        args=dict(output=name,region=dict(x=0,y=0,width=64,height=64),masks=[dict(x=0,y=0,width=64,height=64)],fps=30,duration=.6)
        sid=result(client.tool('framelet_start_recording',args)[0])['session_id']
        terminal=client.tool('framelet_wait_recording',dict(session_id=sid,until='terminal',timeout_ms=10000))[0]
        saved=result(terminal)['result']['recording']
        assert result(terminal)['result']['state']=='completed', terminal
        text=terminal['content'][0]['text']
        assert 'not been visually approved' in text and str(saved['samples']) in text and str(saved['frames']) in text, text
        for path in [name,saved['output'],str(capture_root/name)]:
            inspected=result(client.tool('framelet_inspect',dict(input=path))[0])
            assert inspected['duration_ms']==saved['duration_ms']
        trace=result(client.tool('framelet_timeline',dict(input=saved['output']))[0])
        assert trace['sample_count']==saved['samples'], trace
        client.tool('framelet_probe_frames',dict(input=name,times=[0,.2,.4],output=prefix+'-probe',include_image=True))
        analysis=client.tool('framelet_analyze',dict(input=name))[0]
        assert result(analysis)['quality']['visual_review_required']
        assert result(analysis)['quality']['findings'] and 'Attention:' in analysis['content'][0]['text']
        assert result(analysis)['quality']['sampling']['measured_action_fps'] is None
        assert 'Measured in action windows:' not in analysis['content'][0]['text']
        prepared=client.tool('framelet_prepare_demo',dict(input=name,output=prefix+'-demo.gif',start=0,end=.3))[0]
        assert result(prepared)['state']=='candidate_saved' and 'visually approved' in prepared['content'][0]['text'], prepared
        client.tool('framelet_inspect',dict(input=result(prepared)['output']))
        for path in ['../outside.gif', str(exe), r'\\server\share\file.gif', 'file.gif:stream']:
            client.tool('framelet_inspect',dict(input=path),ok=False)
        assert not list(actual.glob('.framelet-root-*.tmp')), 'root probe leaked'
        proof['redirected_storage']=actual != capture_root.resolve()
        proof['checks'].append('MCP AppData save -> inspect relative/returned/logical paths -> timeline -> probe -> analyze -> prepare -> inspect; outside reads refused; root probe cleaned')
    finally:
        client.close()
    proof['ok']=True
    (root/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8')
    print(json.dumps(proof),flush=True)


if __name__=='__main__':
    main()

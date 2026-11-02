"""Failure recovery and background-session contract. No external runtime in the product."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('exe')
parser.add_argument('--desktop', action='store_true')
args = parser.parse_args()
exe = str(Path(args.exe).resolve())

with tempfile.TemporaryDirectory(prefix='framelet-safety-') as tmp:
    root = Path(tmp)
    env = {**os.environ, 'FRAMELET_MCP_ROOT': tmp, 'FRAMELET_MCP_READ_ROOTS': '[]'}
    messages = ['[]', '{"jsonrpc":"1.0","id":1,"method":"ping"}', '{broken', 'x' * (1024 * 1024 + 10),
                json.dumps({'jsonrpc':'2.0','id':2,'method':'ping'})]
    proc = subprocess.run([exe, 'mcp'], input='\n'.join(messages)+'\n', capture_output=True, text=True, encoding="utf-8", env=env, timeout=20)
    assert proc.returncode == 0, proc.stderr
    replies = [json.loads(line) for line in proc.stdout.splitlines()]
    assert [x['error']['code'] for x in replies[:4]] == [-32600, -32600, -32700, -32600]
    assert replies[4]['result'] == {}
    print('PASS invalid envelopes, malformed JSON and oversized request recover to ping', flush=True)

    proc = subprocess.Popen([exe, 'mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", env=env)
    seq = 0
    def tool(name, arguments=None, ok=True):
        global seq
        seq += 1
        proc.stdin.write(json.dumps({'jsonrpc':'2.0','id':seq,'method':'tools/call','params':{'name':name,'arguments':arguments or {}}})+'\n')
        proc.stdin.flush()
        reply = json.loads(proc.stdout.readline())['result']
        assert bool(reply.get('isError')) != ok, reply
        return reply.get('structuredContent', {}).get('result')
    def terminal(sid):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            r = tool('framelet_recording_status', {'session_id':sid})
            if r['terminal']:
                return r
            time.sleep(0.03)
        raise AssertionError('recording failed to terminate')
    try:
        context = tool('framelet_context')
        assert context['output_root'].removeprefix('\\\\?\\').lower() == str(root.resolve()).lower()
        for output in ['../escape.gif', 'C:/outside.gif', '//server/share/x.gif', 'file.gif:stream', 'CON.gif', 'trailing.gif.', 'x.png']:
            tool('framelet_start_recording', {'region':{'x':0,'y':0,'width':16,'height':16}, 'output':output}, ok=False)
        for region in [None, {'x':0,'y':0,'width':16,'height':16}]:
            tool('framelet_start_recording', {'region':region,'full_screen':region is not None, 'output':'bad.gif'}, ok=False)
        tool('framelet_inspect', {'input':exe}, ok=False)
        assert list(root.iterdir()) == []
        print('PASS root policy, traversal, network/device paths, ADS, extensions, ambiguous/null target', flush=True)

        if args.desktop:
            from PIL import Image
            desktop = tool('framelet_doctor')['desktop']
            region = {'x':desktop['x'],'y':desktop['y'],'width':64,'height':48}
            # The entire source is masked before encoding; no desktop contents are retained.
            capture = {'region':region,'output':'paused.gif','duration':5,'fps':20,'max_width':32,
                       'masks':[{'x':0,'y':0,'width':64,'height':48}]}
            sid = tool('framelet_start_recording', capture)['session_id']
            deadline = time.monotonic()+5
            while tool('framelet_recording_status', {'session_id':sid}).get('samples', 0)<2:
                assert time.monotonic()<deadline
                time.sleep(0.03)
            tool('framelet_start_recording', {**capture, 'output':'second.gif'}, ok=False)
            tool('framelet_pause_recording', {'session_id':sid})
            deadline = time.monotonic()+5
            while True:
                before = tool('framelet_recording_status', {'session_id':sid})
                if before.get('state') == 'paused': break
                assert not before['terminal'] and time.monotonic()<deadline, before
                time.sleep(0.03)
            time.sleep(0.4)
            after = tool('framelet_recording_status', {'session_id':sid})
            assert before.get('state') == after.get('state') == 'paused', (before, after)
            assert before['samples'] == after['samples'] and abs(after['recorded_ms']-before['recorded_ms'])<=20
            tool('framelet_resume_recording', {'session_id':sid})
            time.sleep(0.15)
            tool('framelet_stop_recording', {'session_id':sid})
            done = terminal(sid)
            assert done['result']['state'] == 'completed', done
            saved = root/'paused.gif'
            data = saved.read_bytes()
            assert tool('framelet_stop_recording', {'session_id':sid}) == done
            assert tool('framelet_cancel_recording', {'session_id':sid}) == done
            assert saved.read_bytes() == data
            with Image.open(saved) as im:
                assert im.size == (32,24)
                assert im.convert('RGB').getpixel((0,0)) == (24,24,24)
            print('PASS background start, concurrency refusal, pause timing, resume, stop, idempotency, mask/scale pixels', flush=True)

            sid = tool('framelet_start_recording', {**capture,'output':'cancel.gif','delay':3})['session_id']
            tool('framelet_cancel_recording', {'session_id':sid})
            assert terminal(sid)['result']['state'] == 'cancelled'
            assert not (root/'cancel.gif').exists()
            sid = tool('framelet_start_recording', {**capture,'output':'deadline.gif','duration':0.3})['session_id']
            assert terminal(sid)['result']['state'] == 'completed'
            sid = tool('framelet_start_recording', {**capture,'output':'eof.gif','delay':3})['session_id']
            proc.stdin.close()
            proc.wait(timeout=12)
            assert not (root/'eof.gif').exists()
            assert not list(root.glob('*.partial'))
            print('PASS cancel before capture, automatic deadline, EOF cancellation and partial cleanup', flush=True)
    finally:
        if proc.poll() is None:
            proc.stdin.close()
            proc.wait(timeout=12)
        assert proc.returncode == 0, proc.stderr.read()
print(json.dumps({'ok':True, 'desktop':args.desktop}))

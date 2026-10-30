"""Behavior checks. Pillow is a development-only independent GIF/PNG decoder."""
import os
import argparse
import json
import pathlib
import subprocess
import tempfile
import time
from PIL import Image

p = argparse.ArgumentParser()
p.add_argument('exe', type=pathlib.Path)
p.add_argument('--desktop', action='store_true')
a = p.parse_args()
exe = str(a.exe.resolve())
checks = []

def run(*args, ok=True):
    p = subprocess.run([exe, *map(str, args)], capture_output=True, text=True, timeout=45)
    assert (p.returncode == 0) == ok, (args, p.returncode, p.stdout, p.stderr)
    return json.loads(p.stdout if ok else p.stderr)

def passed(name):
    checks.append(name)
    print('PASS', name, flush=True)

catalog = run('schema')
assert len(catalog['tools']) == 17
passed('versioned tool catalog')

with tempfile.TemporaryDirectory(prefix='framelet-verify-') as folder:
    folder = pathlib.Path(folder)
    source, edited = folder/'source.gif', folder/'edited.gif'
    frames = [Image.new('RGB', (32, 24), c) for c in ['red', 'green', 'blue']]
    frames[0].save(source, save_all=True, append_images=frames[1:], duration=[100, 200, 300], loop=0, disposal=1)
    info = run('inspect', source)['result']
    assert (info['frames'], info['duration_ms']) == (3, 600)
    result = run('edit', source, '--start', 0.1, '--end', 0.6, '--speed', 2, '--output', edited)['result']
    assert result['duration_ms'] == 250
    image = Image.open(edited)
    assert image.n_frames == 2 and image.size == (32, 24)
    image.seek(0)
    assert image.convert('RGB').getpixel((0, 0))[1] >= 120
    image.seek(1)
    assert image.convert('RGB').getpixel((0, 0))[2] >= 250
    image.close()
    passed('trim, speed, independently decoded pixels and timing')
    digest = edited.read_bytes()
    run('edit', source, '--output', edited, ok=False)
    assert edited.read_bytes() == digest
    run('edit', source, '--speed', 0, '--output', folder/'bad.gif', ok=False)
    run('edit', source, '--start', 99, '--output', folder/'empty.gif', ok=False)
    assert not (folder/'empty.gif').exists()
    passed('overwrite refusal, invalid speed, empty range cleanup')

    requests = [
        {'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18'}},
        {'jsonrpc':'2.0','method':'notifications/initialized'},
        {'jsonrpc':'2.0','id':2,'method':'tools/list'},
        {'jsonrpc':'2.0','id':3,'method':'tools/call','params':{'name':'framelet_inspect','arguments':{'input':str(source)}}},
        {'jsonrpc':'2.0','id':4,'method':'tools/call','params':{'name':'framelet_record','arguments':{'output':str(folder/'x.gif'),'duration':301}}},
        {'jsonrpc':'2.0','id':5,'method':'tools/call','params':{'name':'framelet_shot','arguments':{'output':'x.png','cursor':'false'}}},
        {'jsonrpc':'2.0','id':6,'method':'tools/call','params':{'name':'framelet_inspect','arguments':{'input':str(source),'arbitrary':True}}},
    ]
    p = subprocess.run([exe,'mcp'], env={**os.environ,'FRAMELET_MCP_ROOT':str(folder)}, input='\n'.join(map(json.dumps,requests))+'\n', capture_output=True, text=True, timeout=30)
    assert p.returncode == 0, p.stderr
    responses = [json.loads(line) for line in p.stdout.splitlines()]
    assert len(responses) == 6
    assert responses[0]['result']['protocolVersion'] == '2025-06-18'
    assert len(responses[1]['result']['tools']) == 17
    assert responses[2]['result']['structuredContent']['result']['frames'] == 3
    assert all(x['result']['isError'] for x in responses[3:])
    passed('MCP handshake, notification, discovery, tool result and input errors')

    if a.desktop:
        doctor = run('doctor')
        assert doctor['ok'], doctor
        d = doctor['desktop']
        region = f"{d['x']},{d['y']},160,120"
        shot, recording = folder/'shot.png', folder/'capture.gif'
        run('shot','--region',region,'--output',shot)
        with Image.open(shot) as shot_image:
            assert shot_image.size == (160,120)
        result = run('record','--region',region,'--duration',0.6,'--fps',10,'--output',recording)['result']
        assert abs(result['duration_ms'] - 600) <= 20
        with Image.open(recording) as image:
            assert image.size == (160,120)
        passed('live PNG and timed GIF capture')
        session = f'verify-{time.time_ns()}'
        child = subprocess.Popen([exe,'record','--region',region,'--duration','20','--session',session,'--output',str(folder/'stopped.gif')],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            for _ in range(100):
                stop = subprocess.run([exe,'stop','--session',session],capture_output=True,text=True)
                if stop.returncode == 0: break
                time.sleep(0.05)
            else: raise AssertionError('session never became ready')
            out, err = child.communicate(timeout=20)
            assert child.returncode == 0, err
            assert json.loads(out)['result']['duration_ms'] < 20000
            with Image.open(folder/'stopped.gif') as stopped_image:
                stopped_image.verify()
        finally:
            if child.poll() is None: child.kill(); child.wait()
        passed('named session stop finalizes a decodable GIF')

print(json.dumps({'ok':True,'checks':checks},ensure_ascii=False))

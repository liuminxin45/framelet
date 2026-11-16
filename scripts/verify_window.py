"""Exercise fixed-window selection with an owned window and fully masked captures."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

from verify_performance import Client, result
from PIL import Image, ImageSequence

p=argparse.ArgumentParser();p.add_argument('exe',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
exe=a.exe.resolve();root=a.output.resolve();root.mkdir(parents=True,exist_ok=False)
title='Framelet-window-test-'+uuid.uuid4().hex
script="import tkinter as t; r=t.Tk(); r.title(%r); r.configure(bg='#ff0000'); r.geometry('180x100+20+20'); r.after(400,r.deiconify); r.mainloop()" % title
child=subprocess.Popen([sys.executable,'-c',script],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,
                       creationflags=subprocess.CREATE_NO_WINDOW)
client=None
try:
    client=Client(exe,root)
    until=time.monotonic()+15
    while True:
        windows=result(client.tool('framelet_windows')[0])
        matches=[w for w in windows if w['title']==title]
        if len(matches)==1: break
        assert child.poll() is None and time.monotonic()<until, (matches,child.poll())
        time.sleep(.1)
    window=matches[0];r=window['region'];h=window['handle']
    mask=dict(x=0,y=0,width=r['width'],height=r['height'])
    request=dict(output='window.gif',window_handle=h,masks=[mask],duration=.4,fps=2)
    started=result(client.tool('framelet_start_recording',request)[0])
    assert started['window_selection']['region']==r and started['window_selection']['mode']=='fixed_desktop_region'
    assert 'occlusion' in started['window_selection']['warning']
    terminal=result(client.tool('framelet_wait_recording',dict(session_id=started['session_id'],until='terminal',timeout_ms=10000))[0])
    assert terminal['result']['state']=='completed',terminal
    trace=result(client.tool('framelet_timeline',dict(input='window.gif'))[0])
    assert trace['region']==r
    cli=subprocess.run([str(exe),'record','--window-handle',str(h),'--duration','.4','--fps','2',
        '--mask',f"0,0,{r['width']},{r['height']}",'--output',str(root/'cli-window.gif')],
        capture_output=True,text=True,encoding='utf-8',timeout=20)
    assert cli.returncode==0,cli.stderr
    assert json.loads(cli.stdout)['result']['width']==r['width']
    def check_frames(path, stats, *, preserve):
        with Image.open(path) as im:
            frames=[(f.convert('RGB').tobytes(),f.info['duration']) for f in ImageSequence.Iterator(im)]
        assert len(frames) == stats['frames']
        assert len({pixels for pixels,_ in frames})==1
        assert sum(delay for _,delay in frames)==stats['duration_ms']
        if preserve:
            assert len(frames) >= 2, 'capture must contain repeated samples to exercise the option'
            assert len(frames) == stats['samples']
            assert stats['capture_metrics']['merge_identical'] is False
            assert stats['capture_metrics']['merged_samples'] == 0
        return len(frames)
    separate=result(client.tool('framelet_start_recording',{**request,'output':'separate.gif','duration':3,'fps':10,'merge_identical':False})[0])
    ended=result(client.tool('framelet_wait_recording',dict(session_id=separate['session_id'],until='terminal',timeout_ms=10000))[0])
    assert ended['result']['state']=='completed',ended
    stats=ended['result']['recording']
    count=check_frames(root/'separate.gif',stats,preserve=True)
    timeline=json.loads((root/'separate.timeline.json').read_text(encoding='utf-8'))
    assert len(timeline['samples'])==stats['capture_metrics']['source_samples']
    assert [sample['gif_frame'] for sample in timeline['samples']]==list(range(count))
    edited=result(client.tool('framelet_edit',{'input':'separate.gif','output':'separate-edit.gif','merge_identical':False})[0])
    assert check_frames(root/'separate-edit.gif',edited,preserve=True)==count
    combined=result(client.tool('framelet_edit',{'input':'separate.gif','output':'merged-edit.gif'})[0])
    assert check_frames(root/'merged-edit.gif',combined,preserve=False)==1
    assert combined['duration_ms']==edited['duration_ms']
    cli_separate=subprocess.run([str(exe),'record','--window-handle',str(h),'--duration','3','--fps','10',
        '--mask',f"0,0,{r['width']},{r['height']}",'--merge-identical','false','--output',str(root/'cli-separate.gif')],
        capture_output=True,text=True,encoding='utf-8',timeout=20)
    assert cli_separate.returncode==0,cli_separate.stderr
    check_frames(root/'cli-separate.gif',json.loads(cli_separate.stdout)['result'],preserve=True)
    import tkinter as tk
    cover=tk.Tk()
    try:
        cover.overrideredirect(True)
        cover.attributes('-topmost',True)
        cover.configure(bg='#0000ff')
        cover.geometry('300x250+0+0')
        cover.lift()
        cover.after(1200,cover.quit)
        cover.mainloop()
        shot=result(client.tool('framelet_shot',dict(output='covered.png',region=r))[0])
        with Image.open(root/'covered.png') as image:
            assert image.convert('RGB').getpixel((r['width']//2,r['height']//2))==(0,0,255)
        surface=result(client.tool('framelet_start_recording',dict(output='isolated.gif',window_surface=h,duration=2,fps=5))[0])
        state=result(client.tool('framelet_wait_recording',dict(session_id=surface['session_id'],until='terminal',timeout_ms=10000))[0])
        assert state['result']['state']=='completed',state
        assert state['result']['recording']['capture_metrics']['capture_backend']=='wgc_window'
        with Image.open(root/'isolated.gif') as image:
            for frame in ImageSequence.Iterator(image):
                assert frame.convert('RGB').getpixel((image.width//2,image.height//2))==(255,0,0)
        cli_surface=subprocess.run([str(exe),'record','--window-surface',str(h),'--duration','1',
            '--fps','5','--output',str(root/'cli-isolated.gif')],capture_output=True,text=True,encoding='utf-8',timeout=20)
        assert cli_surface.returncode==0,cli_surface.stderr
        assert json.loads(cli_surface.stdout)['result']['capture_metrics']['capture_backend']=='wgc_window'
        with Image.open(root/'cli-isolated.gif') as image:
            assert image.convert('RGB').getpixel((image.width//2,image.height//2))==(255,0,0)
    finally:
        cover.destroy()
    client.tool('framelet_start_recording',{**request,'output':'bad-type.gif','merge_identical':'false'},ok=False)
    client.tool('framelet_start_recording',{**request,'output':'conflict.gif','full_screen':True},ok=False)
    client.tool('framelet_start_recording',{**request,'output':'missing.gif','window_handle':0},ok=False)
    child.terminate();child.wait(timeout=5)
    client.tool('framelet_start_recording',{**request,'output':'closed.gif'},ok=False)
    assert not (root/'closed.gif').exists()
    client.tool('framelet_start_recording',dict(output='closed-surface.gif',window_surface=h),ok=False)
    assert not (root/'closed-surface.gif').exists()
    print(json.dumps(dict(ok=True,checks=['fixed region and frame merging', 'CLI and MCP WGC preserve occluded window pixels', 'invalid, conflicting and destroyed window refused'])))
finally:
    if client is not None: client.close()
    if child.poll() is None:
        child.terminate();child.wait(timeout=5)

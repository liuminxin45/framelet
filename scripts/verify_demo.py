"""Demo delivery regressions: independent pixels/time, non-invasive anchors and bounded failures."""
import argparse
import bisect
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import time

import psutil
from PIL import Image, ImageDraw
from performance_fixtures import publish_trace, create
from verify_performance import Client, result


def main():
    p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',required=True);a=p.parse_args()
    exe=Path(a.exe).resolve();root=Path(a.output).resolve();root.mkdir(parents=True,exist_ok=False)
    checks=[]
    def run(*args,ok=True):
        proc=subprocess.run([str(exe),*map(str,args)],capture_output=True,text=True,encoding='utf-8',timeout=40)
        assert (proc.returncode==0)==ok,(args,proc.stdout,proc.stderr)
        assert 'panicked at' not in proc.stderr
        return json.loads(proc.stdout if ok else proc.stderr)
    # Same failure shape as the real 32-second capture, but entirely synthetic/public-test-safe.
    raw=root/'waiting.gif';durations=[17780,80,90,250,130,100,40,13760]
    pictures=[]
    for i in range(8):
        im=Image.new('RGB',(160,96),'white');ImageDraw.Draw(im).rectangle((10+i*12,30,20+i*12,45),fill='black');pictures.append(im)
    pictures[0].save(raw,save_all=True,append_images=pictures[1:],duration=durations,loop=0,disposal=1)
    total=sum(durations);starts=[];t=0
    for d in durations:starts.append(t*1000);t+=d
    samples=[]
    for i,t in enumerate(range(0,total*1000,83333)):
        end=min(t+83333,total*1000)
        # Split at encoded-frame boundaries, preserving original sample identity.
        stops=[t,*[v for v in starts if t<v<end],end]
        for lo,hi in zip(stops,stops[1:]):
            samples.append(dict(id=i,seq=i,utc_us=1793680000000000+t,capture_us=t,capture_end_us=t+100,
                coverage_start_us=lo,hold_end_us=hi,media_start_us=lo,media_end_us=hi,
                gif_frame=bisect.bisect_right(starts,lo)-1,cursor=None))
    data=dict(schema=3,capture_fps=12,asset_id='a'*32,session_id='b'*32,gif_sha256=hashlib.sha256(raw.read_bytes()).hexdigest(),
        region=dict(x=0,y=0,width=160,height=96),width=160,height=96,gif_bytes=raw.stat().st_size,
        duration_ms=total,frames=8,samples=samples,events=[],dropped_events=0,warnings=[])
    publish_trace(raw,data)
    original=raw.read_bytes();analysis=run('analyze',raw)['result'];q=analysis['quality']
    assert analysis['decode_passes']==1 and analysis['work']['converted_frames']==8
    assert q['holds']['opening_ms']==17780 and q['holds']['closing_ms']==13760,q
    codes={f['code'] for f in q['findings']};assert {'long_opening_hold','long_closing_hold','low_presentation_fps'}<=codes
    assert q['sampling']['measured_action_fps']>11
    candidate=root/'candidate.gif';saved=run('prepare-demo',raw,'--output',candidate)['result']
    assert saved['visual_review_required'] and saved['state']=='candidate_saved'
    assert saved['recording']['duration_ms']<3000 and saved['recording']['frames']==8,saved
    with Image.open(candidate) as output:
        assert output.n_frames==8
        for i in range(8):output.seek(i);assert output.convert('RGB').tobytes()==pictures[i].tobytes()
    assert raw.read_bytes()==original
    assert not list(root.glob('*.partial')) and not list(root.glob('*.pending.json'))
    before=candidate.read_bytes();run('prepare-demo',raw,'--output',candidate,ok=False);assert candidate.read_bytes()==before
    run('prepare-demo',raw,'--output',root/'bad.gif','--start',5,ok=False)
    run('prepare-demo',raw,'--output',root/'bad.gif','--start',0,'--end',40,ok=False)
    assert not (root/'bad.gif').exists()
    run('edit',candidate,'--speed',2,'--output',root/'sped-up.gif')
    sped=run('analyze',root/'sped-up.gif')['result']['quality']['sampling']
    assert sped['requested_fps']==12 and 11<sped['measured_action_fps']<13,sped
    checks.append('32.23s / 8-frame idle-heavy regression: one-pass action discovery, short candidate, all original pixels, raw preserved and collision/invalid range refusal')
    # Real markers and no business input. Static content is valid when intent is explicit.
    c=Client(exe,root)
    def tool(name,args=None,ok=True):return result(c.tool(name,args,ok=ok)[0]) if ok else c.tool(name,args,ok=False)[0]
    def status(sid):return tool('framelet_recording_status',dict(session_id=sid))
    def wait(sid,predicate,seconds=25):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            value=status(sid)
            if predicate(value):return value
            time.sleep(.02)
        raise AssertionError(value)
    common=dict(region=dict(x=0,y=0,width=64,height=64),masks=[dict(x=0,y=0,width=64,height=64)])
    try:
        schema=run('schema');assert len(schema['tools'])==25 and schema['schema_version']=='4.0'
        recording=tool('framelet_start_recording',{**common,'output':'marked.gif','duration':20});sid=recording['session_id']
        wait(sid,lambda s:s.get('ready'))
        def mark(key,kind,**kwargs):return tool('framelet_mark_recording',dict(session_id=sid,request_id=key,kind=kind,**kwargs))
        tool('framelet_mark_recording',dict(session_id=sid,request_id='bad-end',kind='end'),ok=False)
        mark('opening','begin',label='result')
        wait(sid,lambda s:any(m['request']['request_id']=='opening' and m['state']=='recorded' for m in s['markers']))
        # Same ID cannot inject another observation; conflicting reuse fails.
        mark('opening','begin',label='result')
        tool('framelet_mark_recording',dict(session_id=sid,request_id='opening',kind='point'),ok=False)
        tool('framelet_mark_recording',dict(session_id=sid,request_id='bad-label',kind='point',label='x\n'),ok=False)
        time.sleep(.4)
        mark('closing','end')
        tool('framelet_stop_recording',dict(session_id=sid))
        done=wait(sid,lambda s:s['terminal']);assert done['result']['state']=='completed',done
        assert all(m['state']=='recorded' for m in done['markers']),done
        trace=run('timeline',root/'marked.gif')['result'];assert trace['capture_fps']==30,trace
        assert len(trace['markers'])==2 and all(m['media_us'] is not None for m in trace['markers']),trace
        assert all(m['device']=='marker' and m['point'] is None for m in trace['markers'])
        assert not [e for e in trace['events'] if e['device']!='marker' and e['action']!='move'],trace
        marked=run('prepare-demo',root/'marked.gif','--output',root/'marked-candidate.gif')['result']
        assert marked['source_quality']['trim_basis']=='explicit_markers'
        assert marked['source_quality']['sampling']['measured_action_fps']>=24,marked['source_quality']['sampling']
        assert marked['recording']['frames']==1 and marked['recording']['duration_ms']>=300,marked
        derived=run('timeline',root/'marked-candidate.gif')['result']
        assert derived['session_id']==trace['session_id'] and derived['asset_id']!=trace['asset_id']
        checks.append('default 30 FPS; acknowledged begin/end share observation IDs/clocks; duplicate request safe; no synthetic OS input; static marked candidate and trace remapping')
        # Intent anchors survive rotation with globally unique IDs; no invented per-part pair.
        group=tool('framelet_start_series',{**common,'output_dir':str(root),'name':'marker-series','duration':2.5,'segment_seconds':1})
        gid=group['session_id'];wait(gid,lambda s:s.get('ready'))
        tool('framelet_mark_recording',dict(session_id=gid,request_id='series-begin',kind='begin'))
        wait(gid,lambda s:s.get('series',{}).get('part',0)>=2)
        tool('framelet_mark_recording',dict(session_id=gid,request_id='series-end',kind='end'))
        terminal=wait(gid,lambda s:s['terminal']);assert terminal['result']['state']=='completed',terminal
        assert all(m['state']=='recorded' for m in terminal['markers'])
        directory=Path(group['directory']);index=json.loads((directory/'recording.json').read_text(encoding='utf-8'))
        ids=[];anchors=[]
        for part in index['parts']:
            trace=run('timeline',directory/part['recording']['output'])['result'];anchors.extend(trace['markers'])
            data_part=json.loads((directory/part['recording']['timeline']).read_text(encoding='utf-8'))
            ids.extend(s['id'] for s in data_part['samples']);ids.extend(e['id'] for e in data_part['events'])
        assert len(ids)==len(set(ids)) and len(anchors)==2
        assert anchors[0]['mono_us']<anchors[1]['mono_us']
        analyzed=tool('framelet_analyze',dict(input=str(directory)))
        assert any(any(f['code']=='marker_interval_incomplete' for f in seg['quality']['findings']) for seg in analyzed['segments'])
        checks.append('series rotation preserves marker/native identity and acknowledges cross-part incomplete review ranges without inventing boundaries')

        # Pause/new-marker and post-terminal insertion refusal; worker state remains controllable.
        recording=tool('framelet_start_recording',{**common,'output':'paused.gif','duration':20});sid=recording['session_id']
        wait(sid,lambda s:s.get('ready'));tool('framelet_pause_recording',dict(session_id=sid));wait(sid,lambda s:s['state']=='paused')
        tool('framelet_mark_recording',dict(session_id=sid,request_id='paused',kind='point'),ok=False)
        tool('framelet_cancel_recording',dict(session_id=sid));wait(sid,lambda s:s['terminal'])
        tool('framelet_mark_recording',dict(session_id=sid,request_id='late',kind='point'),ok=False)
        # Suspended worker + queued marker + stop: neither pipe write nor mark acknowledgement blocks host.
        recording=tool('framelet_start_recording',{**common,'output':'frozen.gif','duration':60});sid=recording['session_id']
        wait(sid,lambda s:s.get('ready'));child=next(p for p in psutil.Process(c.p.pid).children() if 'worker-record' in p.cmdline());child.suspend()
        began=time.monotonic()
        for i in range(8):tool('framelet_mark_recording',dict(session_id=sid,request_id='frozen'+str(i),kind='point',label='x'*80))
        tool('framelet_mark_recording',dict(session_id=sid,request_id='full',kind='point'),ok=False)
        # Repeated nonterminal controls exceed the anonymous pipe capacity while its reader is frozen.
        for _ in range(40):
            tool('framelet_pause_recording',dict(session_id=sid));time.sleep(.02)
            tool('framelet_resume_recording',dict(session_id=sid));time.sleep(.02)
        before_stop=time.monotonic();tool('framelet_stop_recording',dict(session_id=sid))
        assert time.monotonic()-before_stop<1 and time.monotonic()-began<6
        assert c.rpc('ping')[0]=={}
        done=wait(sid,lambda s:s['terminal']);assert done['result']['state']=='failed',done
        assert all(m['state']=='unconfirmed' for m in done['markers']),done
        assert not child.is_running()
        recording=tool('framelet_start_recording',{**common,'output':'after-failure.gif','duration':.3})
        assert wait(recording['session_id'],lambda s:s['terminal'])['result']['state']=='completed'
        checks.append('pause/terminal insertion refused; frozen worker with full marker queue remains responsive, times out, reports unconfirmed anchors, reaps child and permits next task')
        # Malicious marker metadata must be rejected without creating output or poisoning analysis service.
        hostile=root/'hostile.gif';hostile.write_bytes(raw.read_bytes());bad=copy.deepcopy(data)
        bad['events']=[dict(marker=dict(request_id='bad',kind='point',label='x'*81),id=10000,device='marker',vk=None,scan=None,flags=0,key_name=None,sample_id=None,offset_us=None,seq=0,utc_us=1793680000000000,mono_us=0,kind=0,point=None,injected=False,wheel=0,media_us=None,gif_frame=None)]
        publish_trace(hostile,bad)
        tool('framelet_prepare_demo',dict(input=str(hostile),output='hostile-output.gif'),ok=False);assert not (root/'hostile-output.gif').exists()
        tool('framelet_analyze',dict(input=str(raw)))
        checks.append('hostile marker sidecar rejected before publication and next analysis succeeds')
        # Interrupt real preparation after its durable pending marker appears.
        corpus=create(root/'interrupt-fixtures');source=corpus['dense'];digest=hashlib.sha256(source.read_bytes()).hexdigest()
        def wait_path(predicate,description,seconds=25):
            deadline=time.monotonic()+seconds
            while time.monotonic()<deadline:
                if predicate():return
                time.sleep(.002)
            raise AssertionError(description)
        aborted=root/'aborted-demo.gif'
        ticket=c.send('tools/call',dict(name='framelet_prepare_demo',arguments=dict(input=str(source),output=str(aborted))))
        wait_path(lambda:aborted.with_suffix('.pending.json').exists(),'preparation never entered durable edit stage')
        assert not aborted.exists(),'fixture preparation finished before cancellation point'
        c.cancel(ticket)
        reply,_,_=c.receive(ticket);assert reply.get('isError'),reply
        assert not aborted.exists(),'cancelled pre-publication edit unexpectedly published'
        recovery=run('recover',aborted)['result'];assert recovery['state']=='incomplete',recovery
        assert hashlib.sha256(source.read_bytes()).hexdigest()==digest
        tool('framelet_prepare_demo',dict(input=str(source),output='after-cancel.gif',start=19.5,end=20))
        checks.append('analysis cancellation during durable edit preserves raw/pending evidence, reports incomplete recovery, reaps analysis worker and next preparation succeeds')
        # Race a foreign sidecar only after writer staging proves the preflight check passed.
        raced=root/'postcheck-failure.gif';foreign=raced.with_suffix('.timeline.json');foreign_bytes=b'{"foreign":true}'
        ticket=c.send('tools/call',dict(name='framelet_prepare_demo',arguments=dict(input=str(source),output=str(raced))))
        wait_path(lambda:bool(list(root.glob(raced.name+'.*.partial'))),'preparation never staged GIF')
        assert not raced.exists(),'fixture preparation finished before publication race'
        with foreign.open('xb') as file:file.write(foreign_bytes)
        reply,_,_=c.receive(ticket);assert not reply.get('isError'),reply
        value=result(reply);assert value['state']=='candidate_saved_review_failed',value
        assert raced.exists() and value['recording']['warnings'] and value['failure']['stage']=='demo.review'
        assert foreign.read_bytes()==foreign_bytes and hashlib.sha256(source.read_bytes()).hexdigest()==digest
        with Image.open(raced) as im:im.verify()
        tool('framelet_prepare_demo',dict(input=str(source),output=str(raced)),ok=False)
        tool('framelet_analyze',dict(input=str(raw)))
        checks.append('foreign sidecar publication race preserves valid candidate and foreign bytes; postcheck reports saved path/structured failure; retry refuses overwrite and next analysis succeeds')

    finally:c.close()
    # Unpaired markers and slow action sampling remain explicit; no guessed output.
    d=copy.deepcopy(data);d['capture_fps']=60
    d['events']=[dict(marker=dict(request_id='unclosed',kind='begin',label=''),id=10000,device='marker',vk=None,scan=None,flags=0,key_name=None,sample_id=None,offset_us=None,seq=0,utc_us=1793680000000000,mono_us=0,kind=0,point=None,injected=False,wheel=0,media_us=0,gif_frame=0)]
    unclosed=root/'unclosed.gif';unclosed.write_bytes(raw.read_bytes());publish_trace(unclosed,d)
    q=run('analyze',unclosed)['result']['quality'];assert q['suggested_trim_ms'] is None
    codes={f['code'] for f in q['findings']};assert {'marker_interval_incomplete','action_sampling_below_target','action_sampling_gap'}<=codes
    run('prepare-demo',unclosed,'--output',root/'unclosed-candidate.gif',ok=False);assert not (root/'unclosed-candidate.gif').exists()
    checks.append('unpaired marker refuses automatic trim; action-window under-sampling detected independently of long static periods')
    proof=dict(ok=True,exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),checks=checks,
        fixture=dict(duration_ms=total,encoded_frames=8,candidate_duration_ms=saved['recording']['duration_ms'],numeric_analysis_decode_passes=analysis['decode_passes']))
    (root/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof),flush=True)


if __name__=='__main__':main()

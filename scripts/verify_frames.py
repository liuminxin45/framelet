"""Independent extraction tests. Pillow is a test dependency, never a runtime requirement."""
import argparse
import json
import pathlib
import subprocess
import tempfile
import hashlib
import uuid
from PIL import Image, ImageDraw
from performance_fixtures import publish_trace

parser = argparse.ArgumentParser()
parser.add_argument('exe', type=pathlib.Path)
args = parser.parse_args()
exe = str(args.exe.resolve())

def run(*args, ok=True):
    p = subprocess.run([exe, *map(str, args)], capture_output=True, text=True, encoding="utf-8", timeout=60)
    assert (p.returncode == 0) == ok, (args, p.stdout, p.stderr)
    return json.loads(p.stdout if ok else p.stderr)

def compare_frames(source, report):
    with Image.open(source) as gif:
        for frame in report['frames']:
            gif.seek(frame['frame_index'])
            rgba = gif.convert('RGBA')
            expected = Image.new('RGBA', gif.size, (0, 0, 0, 255))
            expected.alpha_composite(rgba)
            with Image.open(frame['image']) as actual:
                assert actual.convert('RGB').tobytes() == expected.convert('RGB').tobytes(), frame

with tempfile.TemporaryDirectory(prefix='framelet-frames-') as tmp:
    root = pathlib.Path(tmp)
    source = root/'pages.gif'
    pages = [Image.new('RGB', (64,48), c) for c in ['red','green','blue']]
    pages[0].save(source, save_all=True, append_images=pages[1:], duration=[1000]*3, loop=0, disposal=1)
    report = run('frames', source, '--output', root/'scenes', '--count', 8)['result']
    assert len(report['frames']) == 3
    assert [f['timestamp_ms'] for f in report['frames']] == [0,1000,2000]
    assert report['semantic_analysis'] is False and report['source_frames'] == 3
    compare_frames(source, report)
    assert json.loads(pathlib.Path(report['manifest']).read_text()) == report
    with Image.open(report['contact_sheet']) as sheet:
        assert sheet.size == (768,212)
    print('PASS scene candidates, timestamps, contact sheet and independent pixel decode')

    sparse = run('frames', source, '--output', root/'sparse', '--min-gap', 5)['result']
    assert len(sparse['frames']) == 1
    pages[0].save(root/'variable.gif', save_all=True, append_images=pages[1:], duration=[100,900,2000], disposal=1)
    uniform = run('frames', root/'variable.gif', '--output', root/'uniform', '--mode', 'uniform', '--count', 2)['result']
    assert [f['timestamp_ms'] for f in uniform['frames']] == [0,1000]
    assert uniform['duration_ms'] == 3000
    print('PASS scene spacing and time-weighted uniform sampling')

    for at, index in [(0,0),(0.999,0),(1,1),(1.5,1),(2.999,2)]:
        image_path = root/f'at-{at}.png'
        probe = run('frame-at', source, '--at', at, '--output', image_path)['result']
        assert probe['frame_index'] == index and probe['timestamp_ms'] == index*1000
        with Image.open(image_path) as actual:
            assert actual.convert('RGB').tobytes() == pages[index].tobytes()
    for at in [-1,3,'NaN']:
        run('frame-at', source, '--at='+str(at), '--output', root/'invalid-at.png', ok=False)
        assert not (root/'invalid-at.png').exists()
    print('PASS timestamp probes cover the correct half-open frame interval; invalid/end timestamps publish nothing')

    directory = root/'batch review'
    batch = run('probe', source, '--times', 0, 1.5, 2.999, '--output-dir', directory)['result']
    assert directory.is_dir() and pathlib.Path(batch['contact_sheet']) == directory/'contact-sheet.png'
    assert {p.name for p in directory.iterdir()} == {'contact-sheet.png', 'manifest.json'}
    assert json.loads((directory/'manifest.json').read_text())['frames'] == batch['frames']
    with Image.open(batch['contact_sheet']) as sheet:
        assert sheet.format == 'PNG'
    assert [f['frame_index'] for f in batch['frames']] == [0, 1, 2]
    exported = run('probe', source, '--times', 0, 1.5, 2.999, '--output-dir', root/'batch frames', '--export', 'frames')['result']
    for frame, expected in zip(exported['frames'], pages):
        with Image.open(frame['output']) as actual:
            assert actual.convert('RGB').tobytes() == expected.tobytes()
    sentinel = (directory/'manifest.json').read_bytes()
    run('probe', source, '--times', 0, '--output-dir', directory, ok=False)
    assert (directory/'manifest.json').read_bytes() == sentinel
    for flag in ['--output', '-o']:
        run('probe', source, '--times', 0, flag, root/'old-output', ok=False)
        assert not (root/'old-output').exists()
    run('probe', source, '--times', 3, '--output-dir', root/'bad-batch', ok=False)
    assert not (root/'bad-batch').exists()
    print('PASS probe output directory, independent PNG decode, overwrite refusal, obsolete flag rejection and failure cleanup')

    expected = run('probe', source, '--times', 2.999, 0, 1.5, 0)['result']['frames']
    for times in [('2.999,0,1.5,0',), ('2.999,0', '1.5', '0')]:
        actual = run('probe', source, '--times', *times)['result']
        assert actual['frames'] == expected and actual['decode_passes'] == 1
    assert len(run('probe', source, '--times', ','.join(['0']*64))['result']['frames']) == 64
    for times in ['', '0,,1', '0,', '0,bad', '0,-1', '0,NaN', '0,inf', ','.join(['0']*65)]:
        run('probe', source, '--times='+times, '--output-dir', root/'bad-times', ok=False)
        assert not (root/'bad-times').exists()
    print('PASS space/comma/mixed timestamp input, caller order, duplicates, 64-value bound and invalid-input cleanup')

    # A complex palette must not flicker when a duplicate is encoded as a transparent delta.
    gradient=Image.new('RGB',(64,64))
    gradient.putdata([((i*13)%256,(i*29)%256,(i*47)%256) for i in range(4096)])
    gradient.save(root/'gradient.gif',duration=1000)
    two=root/'gradient-separated.gif'
    result=run('edit',root/'gradient.gif','--output',two,'--range',0,.5,'--range',.5,1,'--merge-identical','false')['result']
    assert result['frames']==2 and result['duration_ms']==1000
    with Image.open(two) as image:
        image.seek(0); first=image.convert('RGB').tobytes()
        image.seek(1); assert image.convert('RGB').tobytes()==first
    merged=run('edit',two,'--output',root/'gradient-merged.gif')['result']
    assert merged['frames']==1 and merged['duration_ms']==1000
    print('PASS optional identical-frame preservation retains composited complex-color pixels and playback time')

    noisy = Image.new('RGB', (128, 96))
    import random
    rng = random.Random(41)
    noisy.putdata([tuple(rng.randrange(256) for _ in range(3)) for _ in range(128*96)])
    noisy.save(root/'noisy.gif', duration=1000)
    normal = run('edit', root/'noisy.gif', '--output', root/'normal.gif')['result']
    budget = normal['bytes'] * 9 // 10
    compact = run('edit', root/'noisy.gif', '--output', root/'compact.gif', '--max-bytes', budget,
                  '--auto-compress')['result']
    assert compact['bytes'] <= budget and compact['compression_color_bits_per_channel'] < 8
    assert [compact[k] for k in ('width','height','duration_ms')] == [normal[k] for k in ('width','height','duration_ms')]
    run('edit', root/'noisy.gif', '--output', root/'impossible.gif', '--max-bytes', 1, '--auto-compress', ok=False)
    assert not (root/'impossible.gif').exists()
    print('PASS bounded opt-in automatic compression and no output on impossible budget')

    edited = root/'multi.gif'
    result = run('edit', source, '--output', edited, '--range', 0.2, 0.8, '--range', 1.2, 2.5,
                 '--crop', '4,5,20,10', '--mask', '6,7,3,2')['result']
    assert result['duration_ms']==1900 and (result['width'],result['height'])==(20,10)
    with Image.open(edited) as gif:
        assert gif.n_frames==3
        for i, color in enumerate([(255,0,0),(0,128,0),(0,0,255)]):
            gif.seek(i); frame=gif.convert('RGB')
            assert frame.getpixel((0,0))==color
            assert frame.getpixel((2,2))==(24,24,24)
    for options in [('--range',0,2,'--range',1,2.5),('--range',0,4),('--range',0,'inf'),
                    ('--crop','60,40,10,10'),('--mask','-1,0,2,2'),('--start',.2,'--range',0,1),
                    ('--range',0.1,0.1000001)]:
        run('edit',source,'--output',root/'bad-edit.gif',*options,ok=False)
        assert not (root/'bad-edit.gif').exists()
    assert not list(root.glob('*.pending.json'))
    print('PASS single-pass multi-range crop/redaction pixels, duration, validation and no partial publication')

    samples = [dict(id=i+1,seq=i,utc_us=10000000+i*1000000,capture_us=i*1000000,
                    capture_end_us=i*1000000+100,coverage_start_us=i*1000000,hold_end_us=(i+1)*1000000,
                    media_start_us=i*1000000,media_end_us=(i+1)*1000000,gif_frame=i,cursor=[10,10]) for i in range(3)]
    def event(id,seq,kind,media,point,device='mouse'):
        sample=media//1000000
        return dict(id=id,seq=seq,device=device,vk=65 if device=='keyboard' else None,
                    scan=30 if device=='keyboard' else None,key_name='A' if device=='keyboard' else None,
                    flags=0,sample_id=sample+1,offset_us=media-sample*1000000,utc_us=10000000+media,
                    mono_us=media,kind=kind,point=point,injected=False,wheel=0,media_us=media,gif_frame=sample)
    events=[event(4,0,513,300000,[6,7]),event(5,1,256,1300000,None,'keyboard'),
            event(6,2,513,2200000,[20,20]),event(7,3,513,2400000,[10,10])]
    trace=dict(schema=3,session_id=uuid.uuid4().hex,asset_id=uuid.uuid4().hex,
        gif_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),region=dict(x=-100,y=30,width=64,height=48),
        width=64,height=48,gif_bytes=source.stat().st_size,duration_ms=3000,frames=3,
        samples=samples,events=events,dropped_events=0,warnings=[])
    publish_trace(source,trace)
    out=root/'traced.gif'
    res=run('edit',source,'--output',out,'--range',.2,.8,'--range',1.2,2.5,
            '--crop','4,5,20,10','--mask','6,7,3,2')['result']
    assert res['timeline'] and res['manifest'] and res['warnings']
    derived=json.loads(out.with_suffix('.timeline.json').read_text(encoding='utf-8'))
    assert derived['session_id']==trace['session_id'] and derived['asset_id']!=trace['asset_id']
    assert derived['region']==dict(x=-96,y=35,width=20,height=10)
    assert [(e['id'],e['point']) for e in derived['events']]==[(4,None),(6,None),(7,[6,5])]
    assert all(e['device']!='keyboard' for e in derived['events'])
    checked=run('timeline',out,'--event-id',7,'--detail','full')['result']
    assert checked['events'][0]['desktop_point']==[-90,40] and checked['events'][0]['gif_point']==[6.0,5.0]
    assert checked['events'][0]['utc_us']==trace['events'][3]['utc_us']
    assert not out.with_suffix('.pending.json').exists()
    print('PASS multi-range timeline identity, coordinate remapping, keyboard redaction and manifest validation')

    marked=root/'marked.gif'
    run('edit',source,'--output',marked,'--mask','6,7,3,2','--mask','18,10,3,2','--highlight-clicks')
    assert run('inspect',marked)['result']['duration_ms']==3000
    for time_value,point,expected_color in [(0.35,(14,7),(255,0,0)),(2.25,(28,20),(255,80,40)),(2.45,(18,10),(24,24,24)),(2.8,(28,20),(0,0,255))]:
        picture=root/f'marked-{time_value}.png'
        run('frame-at',marked,'--at',time_value,'--output',picture)
        with Image.open(picture) as im: assert im.convert('RGB').getpixel(point)==expected_color
    assert run('timeline',marked,'--event-id',6)['result']['events'][0]['id']==6
    run('edit',edited,'--output',root/'no-trace.gif','--highlight-clicks',ok=False)
    assert not (root/'no-trace.gif').exists()
    print('PASS opt-in click rings follow observed events, honor redaction and preserve playback duration')

    for input_gif in [source,edited]:
        before=hashlib.sha256(input_gif.read_bytes()).hexdigest()
        run('edit',input_gif,'--output',root/'too-large.gif','--max-bytes','1',ok=False)
        assert not (root/'too-large.gif').exists() and not (root/'too-large.pending.json').exists()
        assert hashlib.sha256(input_gif.read_bytes()).hexdigest()==before
    limited=run('edit',source,'--output',root/'limited.gif','--max-bytes','100000')['result']
    assert limited['bytes']<=100000 and pathlib.Path(limited['output']).is_file()
    print('PASS output byte budget refuses publication, cleans owned staging and preserves original')

    compared=run('frame-at', source, '--at', 1.5, '--reference', root/'at-0.png', '--compare-region', '2,3,10,8', '--output', root/'compared.png')['result']['comparison']
    assert compared['pixels']==80 and compared['changed_pixels']==80 and compared['changed_fraction']==1
    assert abs(compared['mean_absolute_error']-(255+128)/3)<0.001
    run('frame-at', source, '--at', 1.5, '--compare-region', '0,0,10,8', '--output', root/'bad-compare.png', ok=False)
    run('frame-at', source, '--at', 1.5, '--reference', root/'at-0.png', '--compare-region', '60,40,10,10', '--output', root/'bad-compare.png', ok=False)
    assert not (root/'bad-compare.png').exists()
    print('PASS native reference/region difference and invalid-region no-publication')

    delta = []
    for n, color in enumerate(['red','blue','green','yellow']):
        frame = Image.new('RGBA',(48,32),(0,0,0,0))
        ImageDraw.Draw(frame).rectangle((n*8,4,n*8+12,18), fill=color)
        delta.append(frame)
    path = root/'delta.gif'
    delta[0].save(path, save_all=True, append_images=delta[1:], duration=[100]*4, disposal=[1,3,2,1], optimize=True)
    composited = run('frames', path, '--output', root/'delta', '--mode', 'uniform', '--count', 64)['result']
    assert len(composited['frames']) == 4
    compare_frames(path, composited)
    trimmed = root/'delta-trimmed.gif'
    run('edit', path, '--output', trimmed)
    with Image.open(path) as original, Image.open(trimmed) as encoded:
        assert encoded.n_frames == original.n_frames
        for i in range(original.n_frames):
            original.seek(i); encoded.seek(i)
            rgba = original.convert('RGBA')
            expected = Image.new('RGBA', original.size, (0,0,0,255)); expected.alpha_composite(rgba)
            actual = encoded.convert('RGB')
            assert max(abs(a-b) for a,b in zip(expected.convert('RGB').tobytes(),actual.tobytes())) <= 4
    print('PASS cropped/transparency frames and Previous/Background disposal reconstruction')

    sentinel = pathlib.Path(report['manifest']).read_bytes()
    run('frames', source, '--output', root/'scenes', ok=False)
    assert pathlib.Path(report['manifest']).read_bytes() == sentinel
    run('frames', source, '--output', root/'bad', '--count', 65, ok=False)
    run('frames', source, '--output', root/'bad', '--min-change', 2, ok=False)
    broken = root/'broken.gif'; broken.write_bytes(b'GIF89a broken')
    run('frames', broken, '--output', root/'broken', ok=False)
    assert not (root/'bad').exists() and not (root/'broken').exists()
    assert not list(root.glob('.framelet-*.partial'))
    print('PASS overwrite refusal, bounded options, malformed input and failure cleanup')
print(json.dumps({'ok':True,'frame_extraction_checks':12}))

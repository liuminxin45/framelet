"""Deterministic synthetic media/input data; never reads a user's desktop or real keys."""
import bisect
import hashlib
import json
from pathlib import Path
from PIL import Image, ImageDraw


def create(root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    palette = [c for i in range(256) for c in (i, (i * 3) % 256, (i * 7) % 256)]
    base = Image.new('P', (640, 360), 8)
    base.putpalette(palette)
    draw = ImageDraw.Draw(base)
    for row in range(24):
        draw.text((12, row * 14 + 5), f'{row:02d} framelet.record(); agent.verify();', fill=220)
    frames = []
    for i in range(600):
        f = base.copy()
        d = ImageDraw.Draw(f)
        x = i * 5 % 600
        d.rectangle((x, 320, x + 20, 330), fill=180 + i % 60)
        d.rectangle((560, 0, 639, 20), fill=10 + i % 120)
        frames.append(f)
    path = root / 'ui.gif'
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=[30, 30, 40] * 200,
                   loop=0, optimize=False, disposal=1)
    del frames
    with Image.open(path) as im:
        assert im.n_frames == 600
        times = []
        t = 0
        for i in range(im.n_frames):
            im.seek(i)
            times.append((t, t + im.info['duration']))
            t += im.info['duration']
    assert t == 20000
    dense = root / 'dense.gif'
    dense.write_bytes(path.read_bytes())
    clock = 1793610000000000
    samples = [dict(id=i + 1, seq=i, utc_us=clock + a * 1000, capture_us=a * 1000,
                    capture_end_us=a * 1000 + 100, coverage_start_us=a * 1000,
                    hold_end_us=b * 1000, media_start_us=a * 1000, media_end_us=b * 1000,
                    gif_frame=i, cursor=[20, 20]) for i, (a, b) in enumerate(times)]
    starts = [a * 1000 for a, _ in times]
    events = []
    for i in range(20000):
        t = i * 1000
        frame = bisect.bisect_right(starts, t) - 1
        events.append(dict(id=1000000 + i, device='mouse', vk=None, scan=None, flags=0, key_name=None,
                           sample_id=frame + 1, offset_us=t - starts[frame], seq=i, utc_us=clock + t,
                           mono_us=t, kind=513 if i == 12300 else 512, point=[20, 20],
                           injected=False, wheel=0, media_us=t, gif_frame=frame))
    data = dict(schema=3, asset_id='a' * 32, gif_sha256=hashlib.sha256(dense.read_bytes()).hexdigest(),
                session_id='b' * 32, region=dict(x=0, y=0, width=640, height=360), width=640,
                height=360, gif_bytes=dense.stat().st_size, duration_ms=20000, frames=600,
                samples=samples, events=events, dropped_events=0, warnings=[])
    publish_trace(dense, data)
    a = Image.new('RGB', (640, 360), 'black')
    b = a.copy()
    ImageDraw.Draw(b).rectangle((5, 5, 12, 12), fill='red')
    transient = root / 'transient.gif'
    a.save(transient, save_all=True, append_images=[b, a.copy()], duration=[500, 40, 500], loop=0)
    reverse = root / 'reverse.gif'
    a.save(reverse, save_all=True, append_images=[Image.new('RGB', a.size, 'white'), a.copy()],
           duration=[1000, 1000, 1000], loop=0)
    return dict(ui=path, dense=dense, transient=transient, reverse=reverse, data=data)


def publish_trace(gif, data):
    side = gif.with_suffix('.timeline.json')
    side.write_text(json.dumps(data, separators=(',', ':')), encoding='utf-8')
    manifest = dict(schema=1, state='complete', session_id=data['session_id'], asset_id=data['asset_id'],
                    gif_sha256=data['gif_sha256'], timeline_sha256=hashlib.sha256(side.read_bytes()).hexdigest())
    gif.with_suffix('.capture.json').write_text(json.dumps(manifest), encoding='utf-8')


def review_corpus(root):
    """Reproducible review corpus; static long holds are synthetic, not desktop capture."""
    import io
    import shutil
    root = Path(root)
    create(root)
    shutil.copy2(Path(__file__).resolve().parents[1]/'docs/images/bilibili-demo.gif', root/'bilibili.gif')
    group = root/'long.framelet'
    group.mkdir()
    image = Image.new('P', (64,64), 0)
    image.putpalette([0]*768)
    buffer = io.BytesIO()
    image.save(buffer, format='GIF', duration=5000, optimize=False)
    raw = buffer.getvalue()
    split = raw.index(b'!\xf9\x04')
    gif = group/'benchmark.part-000001.gif'
    gif.write_bytes(raw[:split] + raw[split:-1]*60 + b';')
    with Image.open(gif) as check:
        assert check.n_frames == 60
    clock = 1793610000000000
    samples = [dict(id=i+1, seq=i, utc_us=clock+i*1000000, capture_us=i*1000000,
                    capture_end_us=i*1000000+100, coverage_start_us=i*1000000,
                    hold_end_us=(i+1)*1000000, media_start_us=i*1000000,
                    media_end_us=(i+1)*1000000, gif_frame=i//5, cursor=None) for i in range(300)]
    data = dict(schema=3, asset_id='a'*32, gif_sha256=hashlib.sha256(gif.read_bytes()).hexdigest(),
                session_id='b'*32, region=dict(x=0,y=0,width=64,height=64), width=64,height=64,
                gif_bytes=gif.stat().st_size,duration_ms=300000,frames=60,samples=samples,
                events=[],dropped_events=0,warnings=[])
    publish_trace(gif,data)
    stats = dict(output=gif.name,width=64,height=64,frames=60,samples=300,duration_ms=300000,
                 bytes=gif.stat().st_size,timeline=gif.with_suffix('.timeline.json').name,
                 manifest=gif.with_suffix('.capture.json').name,warnings=[])
    index = dict(schema=1,session_id='b'*32,prefix='benchmark',state='completed',reason='fixture',
                 owner_pid=0,owner_created=0,segment_seconds=300,request_id=None,request={},
                 parts=[dict(number=1,start_ms=0,reason='fixture',observation_range=[1,300],
                             utc_range=[clock,clock+300000000],recording=stats)],failure=None)
    (group/'recording.json').write_text(json.dumps(index),encoding='utf-8')

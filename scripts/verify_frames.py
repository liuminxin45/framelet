"""Independent extraction tests. Pillow is a test dependency, never a runtime requirement."""
import argparse
import json
import pathlib
import subprocess
import tempfile
from PIL import Image, ImageDraw

parser = argparse.ArgumentParser()
parser.add_argument('exe', type=pathlib.Path)
args = parser.parse_args()
exe = str(args.exe.resolve())

def run(*args, ok=True):
    p = subprocess.run([exe, *map(str, args)], capture_output=True, text=True, timeout=60)
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
print(json.dumps({'ok':True,'frame_extraction_checks':5}))

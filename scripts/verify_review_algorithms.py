"""Observable review regressions using synthetic media; no desktop or input capture."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
from PIL import Image, ImageDraw
from performance_fixtures import create, publish_trace
from verify_performance import Client, result


def make_group(root, fixture):
    group = root / 'review.framelet'
    group.mkdir()
    parts = []
    for number, start in [(1, 0), (2, 25000)]:
        name = f'review.part-{number:06}.gif'
        gif = group / name
        shutil.copy2(fixture['ui'], gif)
        data = copy.deepcopy(fixture['data'])
        data['asset_id'] = ('a' if number == 1 else 'c') * 32
        shift = (number - 1) * 2000000
        for sample in data['samples']:
            sample['id'] += shift
            for key in ['utc_us', 'capture_us', 'capture_end_us', 'coverage_start_us', 'hold_end_us']:
                sample[key] += start * 1000
        for event in data['events']:
            event['id'] += shift
            event['sample_id'] += shift
            event['utc_us'] += start * 1000
            event['mono_us'] += start * 1000
        publish_trace(gif, data)
        parts.append(dict(number=number, start_ms=start, reason='test', observation_range=None, utc_range=None,
                          recording=dict(output=name, width=640, height=360, frames=600, samples=600,
                                         duration_ms=20000, bytes=gif.stat().st_size,
                                         timeline=gif.with_suffix('.timeline.json').name,
                                         manifest=gif.with_suffix('.capture.json').name, warnings=[])))
    index = dict(schema=1, session_id='b'*32, prefix='review', state='completed', reason='test',
                 owner_pid=0, owner_created=0, segment_seconds=300, request_id=None,
                 request={}, parts=parts, failure=None)
    (group/'recording.json').write_text(json.dumps(index), encoding='utf-8')
    return group


def main():
    p = argparse.ArgumentParser()
    p.add_argument('exe')
    p.add_argument('--output', required=True)
    a = p.parse_args()
    exe = Path(a.exe).resolve()
    root = Path(a.output).resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixture = create(root/'fixtures')
    group = make_group(root, fixture)
    proof = dict(ok=False, exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(), checks=[])
    client = Client(exe, root)
    try:
        times = [25, 0, 19.99, 25.1, 44.99, 0]
        value, _, _ = client.tool('framelet_probe_frames', dict(input=str(group), times=times))
        data = result(value)
        assert data['work']['index_reads'] == 1 and data['work']['segment_validations'] == 2, data
        assert data['decode_passes'] == 2
        for index, (time, frame) in enumerate(zip(times, data['frames'])):
            start = 25000 if time >= 25 else 0
            local = time - start / 1000
            path = group/f"review.part-{2 if start else 1:06}.gif"
            direct, _, _ = client.tool('framelet_probe_frames', dict(input=str(path), times=[local]))
            reference = result(direct)['frames'][0]
            assert frame['request_index'] == index and frame['global_requested_ms'] == int(time * 1000)
            for key in ['frame_index', 'timestamp_ms', 'duration_ms']:
                assert frame[key] == reference[key], (frame, reference)
        value, _, _ = client.tool('framelet_probe_frames', dict(input=str(group), times=[.5]*64))
        assert result(value)['work']['segment_validations'] == 1
        client.tool('framelet_probe_frames', dict(input=str(group), times=[20]), ok=False)
        client.tool('framelet_probe_frames', dict(input=str(group), times=[22]), ok=False)
        client.tool('framelet_probe_frames', dict(input=str(group), times=[45]), ok=False)
        side = group/'review.part-000001.timeline.json'
        original = side.read_bytes()
        side.write_bytes(original.replace(b'"kind":512', b'"kind":513', 1))
        client.tool('framelet_probe_frames', dict(input=str(group), times=[.5]*64), ok=False)
        side.write_bytes(original)
        client.tool('framelet_probe_frames', dict(input=str(group), times=[0]))
        value, _, _ = client.tool('framelet_extract_frames', dict(input=str(group), output=str(root/'group-sheet'), count=6, mode='uniform'))
        overview = result(value)
        assert overview['work']['index_reads'] == 1 and overview['work']['segment_validations'] == 2
        assert overview['decode_passes'] == 2 and len(overview['frames']) == 6 and not overview['missing']
        proof['checks'].append('one index and validation per unique segment; duplicate/order/boundary/gap/tamper/next-request behavior')
        value, _, _ = client.tool('framelet_analyze', dict(input=str(fixture['dense']), start=19.5, end=20))
        assert result(value)['work']['decoded_frames'] == 600
        assert result(value)['work']['converted_frames'] == 16
        palette = [0,0,0, 255,0,0, 0,255,0, 255,255,255] + [0]*756
        frames = []
        for index in range(5):
            frame = Image.new('P', (32,24), 0)
            frame.putpalette(palette)
            ImageDraw.Draw(frame).rectangle((index*4, 3, index*4+7, 12), fill=1+index%3)
            frames.append(frame)
        source = root/'disposal.gif'
        frames[0].save(source, save_all=True, append_images=frames[1:],
                       duration=[70,160,30,200,90], transparency=0,
                       disposal=[1,2,3,1,2], optimize=False)
        full, _, _ = client.tool('framelet_analyze', dict(input=str(source), padding=0))
        peaks = result(full)['candidates']
        for start, end in [(0,.07),(.07,.26),(.09,.48),(.26,.55)]:
            value, _, _ = client.tool('framelet_analyze', dict(input=str(source), start=start, end=end, padding=0))
            expected = [p for p in peaks if int(start*1000) <= p['timestamp_ms'] < int(end*1000)]
            assert result(value)['candidates'] == expected, (start, result(value), expected)
        output = root/'trimmed.gif'
        value, _, _ = client.tool('framelet_edit', dict(input=str(source), output=str(output), start=.09, end=.48))
        assert result(value)['work']['converted_frames'] == 4
        assert result(value)['duration_ms'] == 390
        def pixels(path):
            spans = []
            at = 0
            with Image.open(path) as image:
                for index in range(image.n_frames):
                    image.seek(index)
                    end = at + image.info['duration']
                    spans.append((at, end, image.convert('RGB').tobytes()))
                    at = end
            return spans
        original = pixels(source)
        for at, end, rgb in pixels(output):
            expected = next(p[2] for p in original if p[0] <= at+90 < p[1])
            assert rgb == expected, (at, end)
        proof['checks'].append('window skips prefix RGB, preserves predecessor scores, variable delays and independent transparent/disposal edit pixels')
        value, _, _ = client.tool('framelet_analyze', dict(input=str(source), output=str(root/'candidate-sheet')))
        preview = result(value)
        assert preview['decode_passes'] == 1 and preview['preview_work']['decoded_frames'] == 0
        assert preview['preview_work']['peak_thumbnail_bytes'] <= 32*240*180*3
        assert preview['review_frames'][0]['frame_index'] == 0
        assert preview['review_frames'][-1]['frame_index'] == 4
        assert len({f['frame_index'] for f in preview['review_frames']}) == len(preview['review_frames'])
        # Unscaled small thumbnails must match Pillow's independent compositing.
        with Image.open(preview['contact_sheet']) as sheet:
            columns = min(4, len(preview['review_frames']))
            for frame in preview['review_frames']:
                tile = frame['sheet_tile']-1
                x, y = tile%columns*256+8, tile//columns*212+24
                actual = sheet.crop((x,y,x+32,y+24)).convert('RGB').tobytes()
                assert actual == original[frame['frame_index']][2]
        value, _, _ = client.tool('framelet_analyze', dict(input=str(fixture['transient']), output=str(root/'transient-sheet')))
        assert any(f['frame_index'] == 1 for f in result(value)['review_frames']), result(value)
        full, _, _ = client.tool('framelet_analyze', dict(input=str(fixture['ui']), count=16))
        value, _, _ = client.tool('framelet_analyze', dict(input=str(fixture['ui']), output=str(root/'bounded-sheet'), count=16))
        preview = result(value)
        assert preview['work']['decoded_frames'] == 600 and preview['preview_work']['decoded_frames'] == 0
        assert preview['preview_work']['peak_thumbnail_bytes'] <= 32*240*180*3
        assert preview['review_frames'][0]['frame_index'] == 0 and preview['review_frames'][-1]['frame_index'] == 599
        for key in ['ranges','changed_frames','input_actions','candidates']:
            assert preview[key] == result(full)[key], key
        client.tool('framelet_analyze', dict(input=str(source), output=str(root/'candidate-sheet')), ok=False)
        client.tool('framelet_analyze', dict(input=str(source), start=.0002, end=.0003), ok=False)
        assert not list(root.glob('.framelet-review-*.partial'))
        client.tool('framelet_analyze', dict(input=str(source)))
        proof['checks'].append('single-pass bounded preview preserves context, distinct chronological tiles, independent pixels and 40ms transient; collisions refuse publication')
        proof['ok'] = True
    finally:
        client.close()
        (root/'verification.json').write_text(json.dumps(proof, indent=2), encoding='utf-8')
    print(json.dumps(proof))


if __name__ == '__main__':
    main()

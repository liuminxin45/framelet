"""Observable review regressions using synthetic media; no desktop or input capture."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
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
        proof['checks'].append('one index and validation per unique segment; duplicate/order/boundary/gap/tamper/next-request behavior')
        proof['ok'] = True
    finally:
        client.close()
        (root/'verification.json').write_text(json.dumps(proof, indent=2), encoding='utf-8')
    print(json.dumps(proof))


if __name__ == '__main__':
    main()

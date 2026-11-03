"""P0–P4 observable acceptance: work bounds, recall, async control and current binary evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import statistics
import subprocess
import threading
import time

import psutil
from PIL import Image
from performance_fixtures import create, publish_trace


class Client:
    def __init__(self, exe, root, *, capture_root=None):
        self.seq = 0
        self.q = queue.Queue()
        self.backlog = {}
        self.log = (root / 'mcp.log').open('w', encoding='utf-8')
        self.p = subprocess.Popen([str(exe), 'mcp'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=self.log, env={**os.environ, 'FRAMELET_MCP_ROOT': str(capture_root if capture_root is not None else root),
                                                       'FRAMELET_MCP_READ_ROOTS': '[]'})
        def read():
            for line in self.p.stdout:
                self.q.put((time.perf_counter(), len(line), json.loads(line)))
            self.q.put(None)
        threading.Thread(target=read, daemon=True).start()
        self.rpc('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {},
                               'clientInfo': {'name': 'performance-acceptance', 'version': '1'}})

    def send(self, method, params=None):
        self.seq += 1
        t = time.perf_counter()
        self.p.stdin.write(json.dumps(dict(jsonrpc='2.0', id=self.seq, method=method,
                                         params=params or {})).encode() + b'\n')
        self.p.stdin.flush()
        return self.seq, t

    def receive(self, ticket, timeout=45):
        sid, began = ticket
        until = time.monotonic() + timeout
        while sid not in self.backlog:
            record = self.q.get(timeout=max(.01, until - time.monotonic()))
            assert record is not None, 'MCP exited before response'
            _, _, reply = record
            if 'id' in reply:
                self.backlog[reply['id']] = record
        ended, size, response = self.backlog.pop(sid)
        assert 'error' not in response, response
        return response['result'], (ended - began) * 1000, size

    def rpc(self, method, params=None):
        return self.receive(self.send(method, params))

    def tool(self, name, args=None, ok=True):
        value, ms, size = self.rpc('tools/call', dict(name=name, arguments=args or {}))
        assert bool(value.get('isError')) != ok, value
        return value, ms, size

    def cancel(self, ticket):
        self.p.stdin.write(json.dumps(dict(jsonrpc='2.0', method='notifications/cancelled',
                                         params={'requestId': ticket[0]})).encode() + b'\n')
        self.p.stdin.flush()

    def close(self):
        children = psutil.Process(self.p.pid).children(recursive=True)
        self.p.stdin.close()
        try:
            self.p.wait(timeout=22)
        except subprocess.TimeoutExpired:
            self.p.kill()
            self.p.wait(timeout=5)
            raise
        finally:
            self.log.close()
        _, alive = psutil.wait_procs(children, timeout=6)
        assert not alive, f'orphan analysis/capture process: {[p.pid for p in alive]}'
        assert self.p.returncode == 0


def result(value):
    return value['structuredContent']['result']


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('exe')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    exe = Path(args.exe).resolve()
    root = Path(args.output).resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixtures = create(root / 'fixtures')
    proof = dict(ok=False, exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(), checks=[], metrics={})
    c = Client(exe, root)
    try:
        values = []
        for i in range(6):
            r, ms, size = c.tool('framelet_timeline', {'input': str(fixtures['dense']), 'at': 12.3})
            data = result(r)
            assert any(e['id'] == 1012300 for e in data['events']), data
            assert size < 4096 and not any(b['type'] == 'image' for b in r['content']), size
            values.append(ms)
        assert values[0] < 10000, values
        assert statistics.median(values[1:]) < 250, values
        proof['metrics']['dense_query_ms'] = values
        proof['metrics']['dense_query_wire_bytes'] = size
        proof['checks'].append('5MB dense trace: click recall, warm cache, compact output')

        params = dict(input=str(fixtures['dense']), time_range=[12, 12.5], include_moves=True, limit=30)
        ids = []
        value, _, _ = c.tool('framelet_timeline', params)
        first_cursor = result(value)['next_cursor']
        while True:
            data = result(value)
            ids.extend(e['id'] for e in data['events'])
            if not data['next_cursor']:
                break
            value, _, _ = c.tool('framelet_timeline', {**params, 'cursor': data['next_cursor']})
        assert len(ids) == len(set(ids)) == 500
        fixtures['data']['events'][12001]['kind'] = 513
        publish_trace(fixtures['dense'], fixtures['data'])
        c.tool('framelet_timeline', {**params, 'cursor': first_cursor}, ok=False)
        proof['checks'].append('complete unique pagination; content-changed cursor refused')

        times = [19.99, 0, .51, 10.01, .51]
        value, batch_ms, _ = c.tool('framelet_probe_frames', dict(input=str(fixtures['ui']), times=times,
                                                          output=str(root / 'batch'), export='frames'))
        data = result(value)
        assert data['decode_passes'] == 1 and data['work']['decoded_frames'] == 600, data
        assert data['work']['converted_frames'] == 4, data
        assert len(data['frames']) == len(times)
        with Image.open(fixtures['ui']) as source:
            for i, frame in enumerate(data['frames']):
                assert frame['requested_ms'] == int(times[i] * 1000)
                source.seek(frame['frame_index'])
                with Image.open(frame['output']) as actual:
                    assert source.convert('RGB').tobytes() == actual.convert('RGB').tobytes()
        proof['metrics']['batch_work'] = data['work']
        proof['metrics']['batch_with_png_ms'] = batch_ms
        value, _, _ = c.tool('framelet_probe_frames', dict(input=str(fixtures['ui']), times=[0, 10, 19.99],
                                                          output=str(root / 'sheet')))
        assert Path(result(value)['contact_sheet']).is_file()
        assert not list((root / 'sheet').glob('frame-*.png'))
        c.tool('framelet_probe_frames', dict(input=str(fixtures['ui']), times=[0, 20],
                                            output=str(root / 'invalid-batch')), ok=False)
        assert not (root / 'invalid-batch').exists()
        assert not list(root.glob('.framelet-review-*.partial'))
        proof['checks'].append('one decode pass, selective RGB, duplicate/order/pixel correctness and staged export')

        output = root / 'transient-scene'
        value, _, _ = c.tool('framelet_extract_frames', dict(input=str(fixtures['transient']), output=str(output)))
        assert any(f['frame_index'] == 1 for f in result(value)['frames'])
        value, _, _ = c.tool('framelet_analyze', dict(input=str(fixtures['transient']),
                                                    output=str(root / 'analysis-sheet'), include_image=True))
        data = result(value)
        assert any(a <= 500 and b >= 540 for a, b in data['ranges']), data
        assert len([b for b in value['content'] if b['type'] == 'image']) == 1
        assert not data['semantic_analysis']
        value, _, _ = c.tool('framelet_analyze', dict(input=str(fixtures['reverse']), padding=.1))
        assert len(result(value)['ranges']) == 2, result(value)
        proof['checks'].append('40ms spatial transient retained; A-B-A yields two action intervals')

        ticket_args = dict(input=str(fixtures['ui']), at=19.99, include_image=False)
        repeated=[]
        for _ in range(4):
            _,ms,_=c.tool('framelet_frame_at', ticket_args)
            repeated.append(ms)
        proof['metrics']['same_late_probe_ms']=repeated
        original = fixtures['ui'].read_bytes()
        stamp = fixtures['ui'].stat()
        fixtures['ui'].write_bytes(b'Z' + original[1:])
        os.utime(fixtures['ui'], ns=(stamp.st_atime_ns, stamp.st_mtime_ns))
        c.tool('framelet_frame_at', ticket_args, ok=False)
        fixtures['ui'].write_bytes(original)
        c.tool('framelet_frame_at', ticket_args)
        proof['checks'].append('same-length/same-mtime GIF mutation cannot reuse stale pixels')

        common = dict(output='controlled.gif', duration=30, fps=5,
                      region=dict(x=0, y=0, width=64, height=64),
                      masks=[dict(x=0, y=0, width=64, height=64)])
        value, _, _ = c.tool('framelet_start_recording', common)
        sid = result(value)['session_id']
        value, _, _ = c.tool('framelet_wait_recording', dict(session_id=sid, until='ready', timeout_ms=10000))
        assert result(value)['ready'] and not result(value)['wait_timed_out'], value
        child = next(p for p in psutil.Process(c.p.pid).children() if 'worker-analyze' in p.cmdline())
        child.suspend()
        analysis = c.send('tools/call', dict(name='framelet_frame_at', arguments=ticket_args))
        # Active work plus two queued requests: overflow fails promptly; queued
        # cancellation must not consume recording/control capacity.
        time.sleep(.05)
        queued = [c.send('tools/call', dict(name='framelet_inspect', arguments=dict(input='controlled.gif'))) for _ in range(2)]
        c.tool('framelet_inspect', dict(input='controlled.gif'), ok=False)
        for ticket in queued:
            c.cancel(ticket)
        abandoned = c.send('tools/call', dict(name='framelet_wait_recording',
                            arguments=dict(session_id=sid, until='terminal', timeout_ms=15000)))
        c.cancel(abandoned)
        assert c.receive(abandoned)[0]['isError']
        status, _, _ = c.tool('framelet_recording_status', dict(session_id=sid))
        assert not result(status)['terminal'], status
        waiting = c.send('tools/call', dict(name='framelet_wait_recording',
                                         arguments=dict(session_id=sid, until='terminal', timeout_ms=15000)))
        latencies = []
        status_latencies = []
        for _ in range(20):
            _, ms, _ = c.rpc('ping')
            latencies.append(ms)
            _, ms, _ = c.tool('framelet_recording_status', dict(session_id=sid))
            status_latencies.append(ms)
        _, stop_ms, _ = c.tool('framelet_stop_recording', dict(session_id=sid))
        done, _, _ = c.receive(waiting)
        assert result(done)['result']['state'] == 'completed', done
        assert max(latencies+status_latencies) < 1000 and stop_ms < 1000, (latencies, status_latencies, stop_ms)
        cancellation_started=time.perf_counter()
        c.cancel(analysis)
        cancelled, _, _ = c.receive(analysis)
        cancellation_ms=(time.perf_counter()-cancellation_started)*1000
        assert cancellation_ms<2000,cancellation_ms
        assert cancelled['isError']
        for ticket in queued:
            assert c.receive(ticket)[0]['isError']
        child.wait(timeout=5)
        value, _, _ = c.tool('framelet_inspect', dict(input='controlled.gif'))
        assert result(value)['frames'] >= 1
        metrics = result(done)['result']['recording']['capture_metrics']
        assert metrics['source_samples'] >= 1 and metrics['capture_us'] > 0 and metrics['exact_palette_frames'] >= 1, metrics
        proof['metrics']['control_ping_ms'] = latencies
        proof['metrics']['control_status_ms'] = status_latencies
        proof['metrics']['analysis_cancel_ms'] = cancellation_ms
        proof['metrics']['stop_ms'] = stop_ms
        proof['metrics']['capture'] = metrics
        proof['checks'].append('suspended analysis cannot block ping/stop/wait; queue overflow/queued cancellation bounded; cancelled wait leaves recording active')

        child = next(p for p in psutil.Process(c.p.pid).children() if 'worker-analyze' in p.cmdline())
        child.suspend()
        oversized = c.send('tools/call', dict(name='framelet_frame_at', arguments={**ticket_args, 'reference': 'x' * 100000}))
        _, ms, _ = c.rpc('ping')
        assert ms < 1000
        c.cancel(oversized)
        failed, _, _ = c.receive(oversized)
        assert failed['isError']
        child.wait(timeout=5)
        c.tool('framelet_inspect', dict(input='controlled.gif'))
        proof['checks'].append('blocked large IPC write remains cancellable; fresh analysis succeeds')
        child=next(p for p in psutil.Process(c.p.pid).children() if 'worker-analyze' in p.cmdline())
        child.suspend()
        c.send('tools/call',dict(name='framelet_frame_at',arguments=ticket_args))
        proof['checks'].append('EOF cancels pending frozen analysis and reaps its process')
        proof['ok'] = True
    finally:
        c.close()
        (root / 'verification.json').write_text(json.dumps(proof, indent=2), encoding='utf-8')
    print(json.dumps(proof), flush=True)


if __name__ == '__main__':
    main()

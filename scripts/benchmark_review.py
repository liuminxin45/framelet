"""Alternating release-to-release review benchmarks. Development dependencies only."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import shutil
import statistics
import threading
import winreg
import psutil
from performance_fixtures import review_corpus
from verify_performance import Client, result

CASES = [
    ('group_spread','framelet_probe_frames',dict(input='long.framelet',times=[i*299/63 for i in range(64)])),
    ('group_duplicate','framelet_probe_frames',dict(input='long.framelet',times=[10]*64)),
    ('analyze_tail','framelet_analyze',dict(input='dense.gif',start=19.5,end=20)),
    ('edit_tail','framelet_edit',dict(input='ui.gif',start=19.5,end=20,output=True)),
    ('ui_sheet','framelet_analyze',dict(input='ui.gif',output=True)),
    ('bilibili_sheet','framelet_analyze',dict(input='bilibili.gif',output=True)),
    ('ui_numeric','framelet_analyze',dict(input='ui.gif')),
]


def measured(client, name, args):
    worker = next(p for p in psutil.Process(client.p.pid).children() if 'worker-analyze' in p.cmdline())
    read_before = worker.io_counters().read_bytes
    samples = []
    stop = threading.Event()
    def watch():
        while not stop.is_set():
            try:
                samples.append(worker.memory_info().rss)
            except psutil.NoSuchProcess:
                break
            stop.wait(.005)
    monitor = threading.Thread(target=watch, daemon=True)
    monitor.start()
    try:
        value, ms, size = client.tool(name, args)
    finally:
        stop.set()
        monitor.join(timeout=1)
    data = result(value)
    return dict(ms=ms, wire_bytes=size,
                logical_read_bytes_including_ipc=worker.io_counters().read_bytes-read_before,
                sampled_peak_worker_rss=max(samples,default=0), work=data.get('work'),
                preview_work=data.get('preview_work'), decode_passes=data.get('decode_passes'))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--before', required=True)
    parser.add_argument('--after', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--runs', type=int, default=10)
    a = parser.parse_args()
    assert 5 <= a.runs <= 30
    root = Path(a.output).resolve()
    root.mkdir(parents=True,exist_ok=False)
    proof = dict(ok=False, runs=a.runs, warmup_runs=1,
                 scope='Local MCP tool response latency; excludes model inference and UI operation; alternating warm medians',
                 os=platform.platform(), ram_bytes=psutil.virtual_memory().total, measurements={})
    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,r'HARDWARE\DESCRIPTION\System\CentralProcessor\0') as key:
        proof['cpu'] = winreg.QueryValueEx(key,'ProcessorNameString')[0].strip()
    clients = {}
    try:
        source = root/'corpus'
        review_corpus(source)
        proof['fixture_sha256'] = {p.relative_to(source).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in source.rglob('*') if p.is_file()}
        for label, path in [('before',a.before),('after',a.after)]:
            exe = Path(path).resolve()
            shutil.copytree(source,root/label)
            client = Client(exe,root/label)
            clients[label] = client
            client.tool('framelet_inspect',dict(input='ui.gif')) # start the supervised worker
            proof[label] = dict(exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),exe_bytes=exe.stat().st_size)
        for case, name, args in CASES:
            rows = dict(before=[],after=[])
            for i in range(a.runs+1):
                for label in (['before','after'] if i%2==0 else ['after','before']):
                    actual = dict(args)
                    if actual.get('output'):
                        actual['output'] = str(root/label/(case+f'-{i}'+('.gif' if name=='framelet_edit' else '')))
                    rows[label].append(measured(clients[label],name,actual))
            summary = {label:dict(warmup=data[0],runs=data[1:],median_ms=statistics.median(r['ms'] for r in data[1:]),
                                  max_ms=max(r['ms'] for r in data[1:])) for label,data in rows.items()}
            summary['reduction_percent'] = 100*(1-summary['after']['median_ms']/summary['before']['median_ms'])
            proof['measurements'][case] = summary
            print(case,round(summary['before']['median_ms'],3),'->',round(summary['after']['median_ms'],3),
                  round(summary['reduction_percent'],2),'%',flush=True)
        proof['ok'] = True
    finally:
        try:
            for client in clients.values():
                client.close()
        finally:
            (root/'benchmark.json').write_text(json.dumps(proof,indent=2),encoding='utf-8')


if __name__ == '__main__':
    main()

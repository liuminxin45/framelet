"""Native integration acceptance in isolated profiles; never changes real client configs."""
import argparse,ctypes,hashlib,json,os,queue,shutil,subprocess,threading,time,tomllib
from pathlib import Path

def main():
    p=argparse.ArgumentParser();p.add_argument('exe');p.add_argument('--output',required=True);a=p.parse_args()
    exe=Path(a.exe).resolve();root=Path(a.output).resolve();root.mkdir(parents=True,exist_ok=False)
    checks=[];env={**os.environ,'PYTHONUTF8':'1','LOCALAPPDATA':str(root/'local')}
    def run(*args,ok=True,binary=exe):
        v=subprocess.run([str(binary),*map(str,args)],capture_output=True,text=True,encoding='utf-8',env=env,timeout=35)
        assert (v.returncode==0)==ok,(args,v.stdout,v.stderr)
        result=json.loads(v.stdout if ok else v.stderr)
        return result.get('result',result)
    for client in ['codex','claude-code','cursor','claude-desktop']:
        profile=root/client;profile.mkdir();config=profile/('config.toml' if client=='codex' else 'client.json');skill=profile/'skill';captures=profile/'captures';captures.mkdir();(captures/'keep.bin').write_bytes(b'keep recording data')
        original='# keep comment\nmodel = "selected"\n[mcp_servers.other]\ncommand="other"\n' if client=='codex' else json.dumps({'projects':{'keep':True},'mcpServers':{'other':{'command':'other'}}})
        config.write_text(original,encoding='utf-8')
        opts=['--client',client,'--config',str(config)]+([] if client=='claude-desktop' else ['--skill-dir',str(skill)])
        installed=run('integrate','install',*opts,'--output-root',captures)
        assert installed['managed'] and installed['handshake']['tools']==23 and not installed['handshake']['capture_started']
        run('integrate','check',*opts)
        before=config.read_bytes();run('integrate','install',*opts,'--output-root',captures);assert config.read_bytes()==before,'idempotent install rewrote config'
        if client=='codex':assert '# keep comment' in config.read_text(encoding='utf-8');data=tomllib.loads(config.read_text(encoding='utf-8'))['mcp_servers']
        else:data=json.loads(config.read_text(encoding='utf-8'))['mcpServers']
        assert data['other']['command']=='other'
        assert Path(data['framelet']['command']).resolve()==exe
        if client!='claude-desktop':
            assert (skill/'SKILL.md').is_file() and (skill/'references/installation.json').is_file()
            (skill/'user-note.txt').write_text('retain user addition')
            saved=(skill/'SKILL.md').read_bytes();(skill/'SKILL.md').write_bytes(saved+b'\nuser edit')
            run('integrate','uninstall',*opts,ok=False);assert config.read_bytes()==before
            (skill/'SKILL.md').write_bytes(saved)
        run('integrate','uninstall',*opts);run('integrate','uninstall',*opts)
        assert (captures/'keep.bin').read_bytes()==b'keep recording data'
        if client!='claude-desktop':assert (skill/'user-note.txt').read_text(encoding='utf-8')=='retain user addition' and not (skill/'SKILL.md').exists()
        value=tomllib.loads(config.read_text(encoding='utf-8')) if client=='codex' else json.loads(config.read_text(encoding='utf-8'))
        assert 'framelet' not in value.get('mcp_servers',value.get('mcpServers',{}))
        checks.append(client+': install/check/idempotency/uninstall preserve unrelated config and recordings')

    profile=root/'adopt-upgrade';profile.mkdir();config=profile/'client.json';skill=profile/'skill'
    original={'command':'previous-registration.exe','args':['mcp'],'env':{'KEEP':'private-value'}}
    config.write_text(json.dumps({'mcpServers':{'framelet':original,'other':{'command':'other'}}}))
    opts=['--client','cursor','--config',str(config),'--skill-dir',str(skill)]
    run('integrate','install',*opts,ok=False);assert json.loads(config.read_text(encoding='utf-8'))['mcpServers']['framelet']==original
    run('integrate','install',*opts,'--adopt-existing')
    newer=root/'新目录 with spaces';newer.mkdir();second=newer/'framelet.exe';shutil.copy2(exe,second)
    run('integrate','install',*opts,binary=second);assert json.loads(config.read_text(encoding='utf-8'))['mcpServers']['framelet']['command']==str(second)
    run('integrate','check',*opts,binary=second)
    run('integrate','uninstall',*opts,binary=second);assert json.loads(config.read_text(encoding='utf-8'))['mcpServers']['framelet']==original
    checks.append('explicit adoption and executable-path upgrade; uninstall restores preexisting server entry')

    # A real Windows sharing violation interrupts publication after durable intent/skill writes.
    profile=root/'interrupted';profile.mkdir();config=profile/'client.json';skill=profile/'skill'
    config.write_text('{"other":true}')
    opts=['--client','cursor','--config',str(config),'--skill-dir',str(skill)]
    kernel=ctypes.WinDLL('kernel32',use_last_error=True);kernel.CreateFileW.argtypes=[ctypes.c_wchar_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p,ctypes.c_ulong,ctypes.c_ulong,ctypes.c_void_p];kernel.CreateFileW.restype=ctypes.c_void_p;kernel.CloseHandle.argtypes=[ctypes.c_void_p]
    handle=kernel.CreateFileW(str(config),0x80000000,1,None,3,0,None);assert handle not in (None,ctypes.c_void_p(-1).value)
    try:run('integrate','install',*opts,ok=False)
    finally:kernel.CloseHandle(handle)
    assert list((profile/'.framelet-integrations').glob('*.pending.json'))
    # Unrelated edits between failure/retry must survive.
    config.write_text('{"other":true,"added_after_interrupt":42}')
    run('integrate','install',*opts);assert json.loads(config.read_text(encoding='utf-8'))['added_after_interrupt']==42
    assert not list((profile/'.framelet-integrations').glob('*.pending.json'))
    run('integrate','uninstall',*opts)
    checks.append('real blocked config publication resumes from journal and preserves intervening unrelated edit')

    profile=root/'invalid';profile.mkdir();config=profile/'config.toml';config.write_text('secret="never-echo-private-value\n')
    bad=run('integrate','install','--client','codex','--config',config,'--skill-dir',profile/'skill',ok=False)
    assert 'never-echo-private-value' not in json.dumps(bad) and config.read_text(encoding='utf-8')=='secret="never-echo-private-value\n'
    assert not list((profile/'.framelet-integrations').glob('*.pending.json'))
    checks.append('invalid config rejected without modifying it or disclosing its line')
    proof={'ok':True,'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'checks':checks,'scope':'isolated native config/skill integration; this alone is not a GUI client acceptance'}
    (root/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
if __name__=='__main__':main()

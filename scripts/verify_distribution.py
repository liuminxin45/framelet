"""Validate exact published bundle bytes, schemas and native entry points."""
import argparse,hashlib,json,stat,subprocess,zipfile,urllib.request
from pathlib import Path
import jsonschema

def extract(archive,target):
    target.mkdir(parents=True,exist_ok=False)
    with zipfile.ZipFile(archive) as z:
        assert len(z.infolist())<2048 and sum(i.file_size for i in z.infolist())<128*1024*1024
        for i in z.infolist():
            path=target/i.filename
            assert path.resolve().is_relative_to(target) and ':' not in i.filename and not stat.S_ISLNK(i.external_attr>>16),i.filename
        z.extractall(target)
def main():
    p=argparse.ArgumentParser();p.add_argument('directory');p.add_argument('--output',required=True);p.add_argument('--mcpb-cli',required=True);p.add_argument('--claude');a=p.parse_args()
    a.mcpb_cli=str(Path(a.mcpb_cli).resolve())
    if a.claude:a.claude=str(Path(a.claude).resolve())
    directory=Path(a.directory).resolve();out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=False)
    schemas=Path(__file__).resolve().parent.parent/'packaging/schemas'
    source=json.loads((schemas/'sources.json').read_text(encoding='utf-8'))
    cache=Path('work/distribution-schema-cache');cache.mkdir(parents=True,exist_ok=True)
    loaded={}
    for name,meta in source.items():
        file=cache/(meta['sha256']+'.json')
        if not file.exists():
            with urllib.request.urlopen(meta['url'],timeout=25) as response:data=response.read(256*1024+1)
            assert len(data)<=256*1024 and hashlib.sha256(data).hexdigest()==meta['sha256'],'Schema source changed'
            file.write_bytes(data)
        assert hashlib.sha256(file.read_bytes()).hexdigest()==meta['sha256']
        loaded[name]=json.loads(file.read_text(encoding='utf-8'))
    server=json.loads((directory/'server.json').read_text(encoding='utf-8'));version=server['version'];stem='framelet-v'+version
    jsonschema.validate(server,loaded['server.schema.json'])
    archive=directory/(stem+'-windows-x64.mcpb');assert server['packages'][0]['fileSha256']==hashlib.sha256(archive.read_bytes()).hexdigest()
    extract(archive,out/'mcpb');manifest=json.loads((out/'mcpb/manifest.json').read_text(encoding='utf-8'))
    assert manifest['server']['type']=='binary' and manifest['compatibility']['platforms']==['win32'] and not manifest['compatibility'].get('runtimes')
    assert manifest['server']['entry_point']=='framelet.exe' and manifest['server']['mcp_config']['args']==['mcp']
    r=subprocess.run([a.mcpb_cli,'validate',str(out/'mcpb/manifest.json')],capture_output=True,text=True,encoding='utf-8',timeout=30);assert r.returncode==0,r.stdout+r.stderr
    digest=hashlib.sha256((out/'mcpb/framelet.exe').read_bytes()).hexdigest()
    portable=directory/(stem+'-windows-x64.zip');extract(portable,out/'portable');assert hashlib.sha256((out/'portable/framelet.exe').read_bytes()).hexdigest()==digest
    extract(directory/(stem+'-codex-plugin.zip'),out/'codex')
    for name in ['plugin.schema.json','mcp.schema.json']:
        filename='plugin.json' if name.startswith('plugin') else 'mcp.json';jsonschema.validate(json.loads((out/'codex'/filename).read_text(encoding='utf-8')),loaded[name])
    assert hashlib.sha256((out/'codex/framelet.exe').read_bytes()).hexdigest()==digest
    assert json.loads((out/'codex/mcp.json').read_text(encoding='utf-8'))['mcpServers']['framelet']['command']=='./framelet.exe'
    extract(directory/(stem+'-claude-plugin.zip'),out/'claude')
    assert (out/'claude/framelet.mcpb').read_bytes()==archive.read_bytes()
    assert json.loads((out/'claude/.claude-plugin/plugin.json').read_text(encoding='utf-8'))['mcpServers']=='./framelet.mcpb'
    if a.claude:
        r=subprocess.run([a.claude,'plugin','validate',str(out/'claude'),'--json','--strict'],capture_output=True,text=True,encoding='utf-8',timeout=40)
        (out/'claude-validation.json').write_text(r.stdout,encoding='utf-8');assert r.returncode==0,r.stdout+r.stderr
    extract(directory/(stem+'-skill.zip'),out/'skill')
    for base in [out/'portable/skills/framelet',out/'codex/skills/framelet',out/'claude/skills/framelet',out/'skill/framelet']:
        for file in ['SKILL.md','references/testing.md','references/demos.md','references/editing.md','agents/openai.yaml']:
            assert (base/file).read_bytes()==(out/'mcpb/skills/framelet'/file).read_bytes(),str(base/file)
    proof={'ok':True,'version':version,'exe_sha256':digest,'mcpb_sha256':server['packages'][0]['fileSha256'],'checks':['exact EXE and skill parity across formats','MCPB official validator','pinned source schemas for portable plugin and registry','Claude plugin strict validator' if a.claude else 'Claude bundle references validated','Windows-only binary without extra runtime declarations'],'gui_installation_claimed':False}
    (out/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
if __name__=='__main__':main()

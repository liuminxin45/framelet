"""Package prebuilt native bytes and skills. Development-only Python; no end-user runtime."""
import argparse,hashlib,json,shutil,zipfile
from pathlib import Path

def archive(root,target):
    staging=target.with_name(target.name+'.partial')
    assert not staging.exists(),staging
    try:
        with zipfile.ZipFile(staging,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
            for p in sorted(root.rglob('*')):
                if p.is_file():
                    info=zipfile.ZipInfo(p.relative_to(root).as_posix(),date_time=(1980,1,1,0,0,0));info.compress_type=zipfile.ZIP_DEFLATED;info.external_attr=0o100644<<16
                    z.writestr(info,p.read_bytes())
        staging.replace(target)
    finally:
        if staging.exists():staging.unlink()
def main():
    parser=argparse.ArgumentParser();parser.add_argument('portable');parser.add_argument('destination');a=parser.parse_args()
    portable=Path(a.portable).resolve();dest=Path(a.destination).resolve();dest.mkdir(parents=True,exist_ok=True)
    repo=Path(__file__).resolve().parent.parent
    version=json.loads(__import__('subprocess').check_output([str(portable/'framelet.exe'),'schema'],encoding='utf-8'))['version']
    stem='framelet-v'+version
    # Separate staging for each format; no compatibility manifests in a native package.
    stage=dest/('distribution-'+__import__('uuid').uuid4().hex);stage.mkdir()
    shutil.copytree(portable,stage/'mcpb')
    manifest=json.loads((repo/'packaging/mcpb/manifest.json').read_text(encoding='utf-8'));manifest['version']=version
    (stage/'mcpb/manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
    mcpb=dest/(stem+'-windows-x64.mcpb');archive(stage/'mcpb',mcpb)
    shutil.copytree(repo/'skills/framelet',stage/'skill/framelet');shutil.copy2(repo/'LICENSE',stage/'skill/framelet/LICENSE');skill=dest/(stem+'-skill.zip');archive(stage/'skill',skill)
    shutil.copytree(portable,stage/'codex');shutil.copy2(repo/'packaging/codex/plugin.json',stage/'codex/plugin.json');shutil.copy2(repo/'packaging/codex/mcp.json',stage/'codex/mcp.json')
    data=json.loads((stage/'codex/plugin.json').read_text(encoding='utf-8'));data['version']=version;(stage/'codex/plugin.json').write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
    codex=dest/(stem+'-codex-plugin.zip');archive(stage/'codex',codex)
    claude=stage/'claude';claude.mkdir();shutil.copytree(repo/'skills',claude/'skills');(claude/'.claude-plugin').mkdir();data=json.loads((repo/'.claude-plugin/plugin.json').read_text(encoding='utf-8'));data.update(version=version,mcpServers='./framelet.mcpb');(claude/'.claude-plugin/plugin.json').write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8');shutil.copy2(mcpb,claude/'framelet.mcpb');shutil.copy2(repo/'LICENSE',claude/'LICENSE')
    claude_zip=dest/(stem+'-claude-plugin.zip');archive(claude,claude_zip)
    server={'$schema':'https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json','name':'io.github.liuminxin45/framelet','title':'Framelet','description':'Tiny native Windows screen recorder with keyboard/mouse/frame sync and GIF review.','version':version,'repository':{'url':'https://github.com/liuminxin45/framelet','source':'github'},'packages':[{'registryType':'mcpb','identifier':'https://github.com/liuminxin45/framelet/releases/download/v'+version+'/'+mcpb.name,'fileSha256':hashlib.sha256(mcpb.read_bytes()).hexdigest(),'transport':{'type':'stdio'}}]}
    (dest/'server.json').write_text(json.dumps(server,indent=2),encoding='utf-8')
    files=[mcpb,skill,codex,claude_zip]
    (dest/'SHA256SUMS-distribution.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n' for p in files),encoding='ascii')
    print(json.dumps({'version':version,'artifacts':[str(p) for p in files],'registry_manifest':str(dest/'server.json'),'staging':str(stage)}))
if __name__=='__main__':main()

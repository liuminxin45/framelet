"""Install actual client bundles in isolated host profiles, then connect their MCP servers."""
import argparse,hashlib,json,os,shutil,subprocess,zipfile
from pathlib import Path
from host_rpc import Host

def main():
    p=argparse.ArgumentParser();p.add_argument('directory');p.add_argument('--output',required=True);p.add_argument('--codex',default='codex.cmd');p.add_argument('--claude',required=True);a=p.parse_args()
    a.codex=str(Path(a.codex).resolve()) if Path(a.codex).is_file() else shutil.which(a.codex)
    assert a.codex,'Codex executable not found'
    a.claude=str(Path(a.claude).resolve())
    directory=Path(a.directory).resolve();out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=False)
    server=json.loads((directory/'server.json').read_text(encoding='utf-8'));stem='framelet-v'+server['version']
    # Sources are our just-validated build outputs, not third-party archives.
    market=out/'codex-market';market.mkdir();plugin=market/'plugins/framelet';plugin.mkdir(parents=True)
    with zipfile.ZipFile(directory/(stem+'-codex-plugin.zip')) as z:z.extractall(plugin)
    catalog=market/'.agents/plugins/marketplace.json';catalog.parent.mkdir(parents=True)
    catalog.write_text(json.dumps({'name':'framelet-package-test','interface':{'displayName':'Framelet package acceptance'},'plugins':[{'name':'framelet','source':{'source':'local','path':'./plugins/framelet'},'policy':{'installation':'AVAILABLE','authentication':'ON_INSTALL'},'category':'Productivity'}]}),encoding='utf-8')
    env={**os.environ,'PYTHONUTF8':'1','CODEX_HOME':str(out/'codex-profile'),'CLAUDE_CONFIG_DIR':str(out/'claude-profile'),'LOCALAPPDATA':str(out/'local'),'APPDATA':str(out/'appdata')};Path(env['CODEX_HOME']).mkdir()
    h=Host([a.codex,'app-server','--stdio'],env,market,out/'codex-install.log')
    try:
        h.initialize();listing=h.call('plugin/list',{'cwds':[str(market)],'marketplaceKinds':['local']})
        (out/'codex-plugin-list.json').write_text(json.dumps(listing,indent=2),encoding='utf-8')
        # Use the returned marketplace identity, never guess a cache or installation ID.
        entries=listing.get('marketplaces',listing.get('data',[]));match=next(x for x in entries if any(p['name']=='framelet' for p in x.get('plugins',[])))
        marketplace=match.get('path',match.get('marketplacePath'))
        assert marketplace,match
        installed=h.call('plugin/install',{'pluginName':'framelet','marketplacePath':marketplace})
        (out/'codex-plugin-install.json').write_text(json.dumps(installed,indent=2),encoding='utf-8')
    finally:h.close()
    h=Host([a.codex,'app-server','--stdio'],env,market,out/'codex-connect.log')
    try:
        h.initialize();status=h.call('mcpServerStatus/list',{'detail':'full'});(out/'codex-status.json').write_text(json.dumps(status,indent=2),encoding='utf-8')
        servers=status.get('data',status.get('servers',[]));connected=[s for s in servers if len(s.get('tools',{}))==23 and 'framelet' in s['name']];assert connected,'Codex plugin MCP did not connect'
        skills=h.call('skills/list',{'cwds':[str(market)],'forceReload':True});assert any('framelet' in s['name'] and s['enabled'] for g in skills['data'] for s in g['skills'])
        codex={'ok':True,'server':connected[0]['name'],'tools':len(connected[0]['tools']),'skill_discovered':True}
    finally:h.close()

    marketplace=out/'claude-market';plugin=marketplace/'plugins/framelet';plugin.mkdir(parents=True)
    with zipfile.ZipFile(directory/(stem+'-claude-plugin.zip')) as z:z.extractall(plugin)
    catalog=marketplace/'.claude-plugin/marketplace.json';catalog.parent.mkdir()
    catalog.write_text(json.dumps({'name':'framelet-package-test','owner':{'name':'Framelet acceptance'},'metadata':{'description':'Isolated package acceptance'},'plugins':[{'name':'framelet','source':'./plugins/framelet'}]}),encoding='utf-8')
    commands=[['plugin','marketplace','add',str(marketplace)],['plugin','install','framelet@framelet-package-test','--config','capture_directory='+str(out/'captures'),'--json'],['mcp','list']]
    results=[]
    for i,args in enumerate(commands):
        r=subprocess.run([a.claude,*args],env=env,cwd=out,capture_output=True,text=True,encoding='utf-8',timeout=90)
        (out/f'claude-{i}.log').write_text(r.stdout+'\n'+r.stderr,encoding='utf-8');assert r.returncode==0,r.stdout+r.stderr;results.append(r.stdout)
    assert 'framelet' in results[-1] and 'Connected' in results[-1],results[-1]
    digest=hashlib.sha256((market/'plugins/framelet/framelet.exe').read_bytes()).hexdigest()
    cached=[p for p in plugin.rglob('framelet.exe') if '.mcpb-cache' in p.parts and hashlib.sha256(p.read_bytes()).hexdigest()==digest];assert cached,'Claude did not extract the exact native binary in its resolved plugin cache'
    proof={'ok':True,'version':server['version'],'exe_sha256':digest,'codex':codex,'claude':{'ok':True,'source':'local plugin marketplace + exact MCPB + user-config directory','native_cache_verified':True},'model_turns':0,'gui_installation_claimed':False}
    (out/'verification.json').write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
if __name__=='__main__':main()

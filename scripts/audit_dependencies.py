"""Query OSV for locked registry packages; no recorder runtime dependency."""
import argparse,json,pathlib,tomllib,urllib.request,sys
p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,default=pathlib.Path('work/dependency-audit.json'));a=p.parse_args()
packages=[p for p in tomllib.loads(pathlib.Path('Cargo.lock').read_text(encoding='utf-8'))['package'] if p.get('source','').startswith('registry+')]
request=urllib.request.Request('https://api.osv.dev/v1/querybatch',data=json.dumps({'queries':[{'package':{'ecosystem':'crates.io','name':p['name']},'version':p['version']} for p in packages]}).encode(),headers={'Content-Type':'application/json','User-Agent':'Framelet-dependency-check'})
try:
 with urllib.request.urlopen(request,timeout=45) as response:results=json.load(response)['results']
 assert len(results)==len(packages)
 findings=[{'name':p['name'],'version':p['version'],'vulns':r['vulns']} for p,r in zip(packages,results) if r.get('vulns')]
 result={'ok':not findings,'packages_checked':len(packages),'findings':findings,'source':'https://api.osv.dev/v1/querybatch'}
except Exception as e:result={'ok':False,'audit_error':str(e),'packages_checked':0}
a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(result,indent=2),encoding='utf-8');print(json.dumps(result));sys.exit(0 if result['ok'] else 1)

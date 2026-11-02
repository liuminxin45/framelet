"""Manual, SHA-bound registry publication using GitHub Actions OIDC (no stored PAT)."""
import argparse,hashlib,json,os,re,subprocess,tarfile,urllib.parse,urllib.request
from pathlib import Path
VERSION='1.8.7'
PUBLISHER_SHA='cdb42bfe6a5e1cba7526e007ac13f13261ed35224386fee8a34ed2c2e8e778d4'
REPO='liuminxin45/framelet'
def fetch(url,limit=64*1024*1024):
    request=urllib.request.Request(url,headers={'User-Agent':'Framelet-registry-publication'})
    with urllib.request.urlopen(request,timeout=30) as r:data=r.read(limit+1)
    assert len(data)<=limit,'HTTP response exceeds publication budget'
    return data
def main():
    p=argparse.ArgumentParser();p.add_argument('--tag',required=True);p.add_argument('--source',required=True);p.add_argument('--package-sha',required=True);p.add_argument('--manifest',default='server.json');p.add_argument('--check-only',action='store_true');a=p.parse_args()
    proof={'ok':False,'tag':a.tag,'source_commit':a.source,'package_sha256':a.package_sha,'publisher_version':VERSION}
    target=Path('work/registry-publication.json');target.parent.mkdir(parents=True,exist_ok=True)
    try:
        assert re.fullmatch(r'v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?',a.tag)
        assert re.fullmatch('[0-9a-f]{40}',a.source) and re.fullmatch('[0-9a-f]{64}',a.package_sha)
        assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==a.source,'Checkout differs from requested source'
        manifest=json.loads(Path(a.manifest).read_text(encoding='utf-8'))
        assert manifest['name']=='io.github.liuminxin45/framelet' and manifest['version']==a.tag[1:]
        package=manifest['packages'][0];expected=f'https://github.com/{REPO}/releases/download/{a.tag}/framelet-{a.tag}-windows-x64.mcpb'
        assert package['registryType']=='mcpb' and package['identifier']==expected and package['fileSha256']==a.package_sha
        tag=json.loads(fetch(f'https://api.github.com/repos/{REPO}/git/ref/tags/{a.tag}',1024*1024));assert tag['object']['type']=='commit' and tag['object']['sha']==a.source,'Release tag differs from source SHA'
        release=json.loads(fetch(f'https://api.github.com/repos/{REPO}/releases/tags/{a.tag}',2*1024*1024));assert not release['draft']
        runs=json.loads(fetch(f'https://api.github.com/repos/{REPO}/actions/runs?head_sha={a.source}&per_page=10',8*1024*1024))['workflow_runs']
        assert any(r['name']=='Windows' and r['head_sha']==a.source and r['conclusion']=='success' for r in runs),'No successful Windows acceptance for the source SHA'
        assert hashlib.sha256(fetch(expected)).hexdigest()==a.package_sha,'Published MCPB bytes differ from manifest'
        if not a.check_only:
            assert os.environ.get('ACTIONS_ID_TOKEN_REQUEST_URL') and os.environ.get('ACTIONS_ID_TOKEN_REQUEST_TOKEN'),'Publication requires the manual GitHub OIDC workflow'
            private=Path(os.environ['RUNNER_TEMP'])/'framelet-registry-publisher';private.mkdir(exist_ok=True)
            payload=fetch(f'https://github.com/modelcontextprotocol/registry/releases/download/v{VERSION}/mcp-publisher_windows_amd64.tar.gz')
            assert hashlib.sha256(payload).hexdigest()==PUBLISHER_SHA,'Publisher digest mismatch'
            archive=private/'publisher.tar.gz';archive.write_bytes(payload)
            with tarfile.open(archive) as z:
                member=next(m for m in z.getmembers() if m.name=='mcp-publisher.exe');assert member.size<100*1024*1024
                binary=private/'mcp-publisher.exe';binary.write_bytes(z.extractfile(member).read())
            for args in [['login','github-oidc'],['publish',str(Path(a.manifest).resolve())]]:
                result=subprocess.run([str(binary),*args],cwd=private,capture_output=True,timeout=90)
                assert result.returncode==0,f'Publisher {args[0]} failed (exit {result.returncode}); credentials/output are not logged'
            name=urllib.parse.quote(manifest['name'],safe='');version=urllib.parse.quote(manifest['version'],safe='')
            url=f'https://registry.modelcontextprotocol.io/v0.1/servers/{name}/versions/{version}'
            published=json.loads(fetch(url,2*1024*1024));server=published.get('server',published)
            assert server['name']==manifest['name'] and server['version']==manifest['version'] and server['packages'][0]['fileSha256']==a.package_sha
            proof['registry_url']=url
        proof.update(ok=True,published=not a.check_only)
    except Exception as e:
        proof['error']=str(e)[:1024]
    target.write_text(json.dumps(proof,indent=2),encoding='utf-8');print(json.dumps(proof))
    if not proof['ok']:raise SystemExit(1)
if __name__=='__main__':main()

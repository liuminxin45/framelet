"""Collect the licenses of dependencies actually resolved by Cargo (stdlib only)."""
import json
import pathlib
import shutil
import subprocess
import sys

destination = pathlib.Path(sys.argv[1]).resolve()
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--filter-platform', 'x86_64-pc-windows-msvc', '--format-version', '1'], text=True, encoding="utf-8"))
packages = [p for p in metadata['packages'] if p['name'] != 'framelet']
notices = ['# Third-party notices', '', 'Framelet links the following open-source Rust dependencies. Their license texts are included in licenses/.', '']
sanitized = []
for p in sorted(packages, key=lambda p: p['name']):
    root = pathlib.Path(p['manifest_path']).parent
    candidates = [f for f in root.iterdir() if f.is_file() and f.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE'))]
    for name in ['licenses', 'license']:
        folder = root / name
        if folder.is_dir():
            candidates.extend(f for f in folder.rglob('*') if f.is_file())
    if p.get('license_file'):
        candidates.append(root / p['license_file'])
    if not candidates:
        raise RuntimeError(f"No license text found for {p['name']} {p['version']}")
    target = destination / 'licenses' / f"{p['name']}-{p['version']}"
    target.mkdir(parents=True, exist_ok=True)
    for file in set(candidates):
        relative=file.resolve().relative_to(root.resolve())
        output=target/relative
        output.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(file,output)
    sanitized.append({k: p.get(k) for k in ['name', 'version', 'license', 'repository', 'source']})
    notices.append(f"- {p['name']} {p['version']} — {p.get('license')} — {p.get('repository') or p.get('source')}")
sysroot = pathlib.Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True, encoding="utf-8").strip())
rust_licenses = destination / 'licenses' / 'rust-standard-library'
rust_licenses.mkdir(parents=True, exist_ok=True)
for file in (sysroot / 'share' / 'doc' / 'rust').glob('*'):
    if file.is_file() and (file.name.upper().startswith('LICENSE') or file.name == 'COPYRIGHT-library.html'):
        shutil.copyfile(file, rust_licenses / file.name)
(destination / 'dependencies.json').write_text(json.dumps(sanitized, indent=2), encoding='utf-8')
(destination / 'THIRD_PARTY_NOTICES.md').write_text('\n'.join(notices) + '\n', encoding='utf-8')
print(f'Collected license texts for {len(packages)} crates')

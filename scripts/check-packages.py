#!/usr/bin/env python3
"""Build unpacked archives together without requiring a published client crate.

Usage: python3 scripts/check-packages.py [--offline]
Requires Python 3.12+, Cargo, and protoc. Never publishes packages.
"""
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tomllib

root = Path(__file__).resolve().parents[1]
version = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version']
subprocess.run(['cargo', 'package', '--workspace', '--allow-dirty', '--no-verify', *sys.argv[1:]], cwd=root, check=True)
verify = root / 'target' / 'package' / 'verified-source'
verify.mkdir(parents=True, exist_ok=True)
for name in ('cliche-client', 'cliche-gateway'):
    with tarfile.open(root / 'target' / 'package' / f'{name}-{version}.crate') as archive:
        names = archive.getnames()
        if not any(name.endswith('/LICENSE') for name in names):
            raise RuntimeError('package is missing its license')
        if any('.sqlite' in name or '/.env' in name or 'node_modules' in name for name in names):
            raise RuntimeError('package contains private/runtime artifacts')
        archive.extractall(verify, filter='data')
patch = verify / 'patch.toml'
patch.write_text('[patch.crates-io]\ncliche-client = { path = ' + json.dumps(str(verify / f'cliche-client-{version}')) + ' }\n')
for name in ('cliche-client', 'cliche-gateway'):
    subprocess.run(['cargo', 'check', '--manifest-path', str(verify / f'{name}-{version}' / 'Cargo.toml'), '--all-targets', '--all-features', '--config', str(patch), '--target-dir', str(root / 'target'), *sys.argv[1:]], cwd=root, check=True)
print('Both package archives passed content and unpacked-source compilation checks.')

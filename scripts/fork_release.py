#!/usr/bin/env python3
"""Assign a fork build identity and generate its immutable release manifest."""

import hashlib
import json
import pathlib
import re
import sys

TARGETS = ('windows-x86_64',)
RELEASES = 'https://github.com/alexshpunt/herdr-core/releases'


def main():
    mode, root, run, attempt = sys.argv[1:]
    if not (run.isascii() and run.isdigit() and int(run) > 0 and attempt.isascii() and attempt.isdigit() and int(attempt) > 0):
        raise ValueError('Invalid fork build identity')
    root = pathlib.Path(root)
    cargo = root / 'crates/herdr-core/Cargo.toml'
    lock = root / 'Cargo.lock'
    source = cargo.read_text(encoding='utf-8')
    match = re.search(r'(?m)^version = "(\d+\.\d+\.\d+)(-fork\.\d+\.\d+)?"$', source)
    if not match:
        raise ValueError('Expected one numeric herdr-core package version')
    base = match.group(1)
    version = f'{base}-fork.{run}.{attempt}'
    current = base + (match.group(2) or '')
    if current != base and current != version:
        raise ValueError('Package has a different fork build identity')
    if mode == 'version':
        if current != base:
            raise ValueError('Package already has a fork build identity')
        cargo.write_text(source[:match.start(1)] + version + source[match.end(1):], encoding='utf-8')
        locked = lock.read_text(encoding='utf-8')
        pattern = r'(?m)(^name = "herdr-core"\nversion = ")' + re.escape(base) + r'("$)'
        updated, count = re.subn(pattern, lambda m: m.group(1) + version + m.group(2), locked)
        if count != 1:
            raise ValueError('Expected one matching herdr-core lockfile entry')
        lock.write_text(updated, encoding='utf-8')
    elif mode == 'manifest':
        api = (root / 'crates/herdr-core/src/api.rs').read_text(encoding='utf-8')
        generation = re.search(r'(?m)^pub const GENERATION: u32 = (\d+);$', api)
        if not generation:
            raise ValueError('Missing protocol generation')
        targets = {}
        for target in TARGETS:
            ext = 'zip' if target.startswith('windows-') else 'tar.gz'
            name = f'herdr-core-{target}.{ext}'
            archive = root / 'dist' / name
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            advertised = archive.with_name(name + '.sha256').read_text(encoding='utf-8').split()[0]
            if checksum != advertised:
                raise ValueError(f'Checksum mismatch: {name}')
            targets[target] = {'url': f'{RELEASES}/download/fork-{run}-{attempt}/{name}', 'sha256': checksum}
        manifest = {'version': version, 'generation': int(generation.group(1)), 'targets': targets}
        (root / 'dist/manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    else:
        raise ValueError('Unknown mode')


if __name__ == '__main__':
    main()

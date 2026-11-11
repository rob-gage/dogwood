#!/usr/bin/env python3
"""Restore or check the checksum-pinned official Windows DXC binaries."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1] / 'third_party/dxc'


def verify(data, expected, machine):
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError('DXC checksum mismatch')
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('Not a PE binary')
    offset = struct.unpack_from('<I', data, 0x3c)[0]
    if data[offset:offset+4] != b'PE\0\0' or struct.unpack_from('<H', data, offset+4)[0] != machine:
        raise ValueError('Wrong DXC architecture')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    manifest = json.loads((ROOT / 'manifest.json').read_text())
    archive = None
    if not args.check:
        with urllib.request.urlopen(manifest['url'], timeout=120) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != manifest['archive_sha256']:
            raise ValueError('DXC archive checksum mismatch')
        archive = zipfile.ZipFile(io.BytesIO(data))
    for target, arch, machine in [('windows-x64', 'x64', 0x8664), ('windows-arm64', 'arm64', 0xaa64)]:
        entry = manifest['compilers'][target]
        path = ROOT / entry['file']
        data = archive.read(f'bin/{arch}/dxcompiler.dll') if archive else path.read_bytes()
        verify(data, entry['sha256'], machine)
        if archive:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    if archive:
        for name in ['LICENSE-LLVM.txt', 'LICENSE-MIT.txt', 'ReleaseNotes.md']:
            (ROOT / name).write_bytes(archive.read(name))
    print('DXC checksums and PE architectures verified')


if __name__ == '__main__':
    main()

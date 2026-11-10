#!/usr/bin/env python3
"""Vendor the latest public LunarG runtime loaders for Windows x64 and ARM64."""
import argparse
import datetime
import hashlib
import json
import re
import struct
import tempfile
import urllib.request
import zipfile
from pathlib import Path

CATALOGUE = 'https://vulkan.lunarg.com/sdk/files.json'
TARGETS = {'windows-x64': ('windows', 0x8664), 'windows-arm64': ('warm', 0xAA64)}


def fetch(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'Dogwood-Vulkan-Updater/1.0'})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def machine(data):
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('not a Windows PE binary')
    offset = struct.unpack_from('<I', data, 0x3C)[0]
    if offset + 6 > len(data) or data[offset:offset + 4] != b'PE\0\0':
        raise ValueError('invalid PE header')
    return struct.unpack_from('<H', data, offset + 4)[0]


def latest_runtime(catalogue, platform):
    for version in sorted(catalogue[platform], key=lambda v: tuple(map(int, v.split('.'))), reverse=True):
        release = catalogue[platform][version]
        if release.get('acl') != 'public':
            continue
        files = [f for f in release['files'] if
                 re.fullmatch(r'VulkanRT-(X64|ARM64)-[0-9.]+-Components\.zip', f['file_name'])
                 and f.get('acl') == 'public' and f.get('uploaded')]
        if len(files) == 1:
            return version, files[0]
    raise ValueError(f'no public runtime component archive for {platform}')


def unpack(archive, expected_machine):
    with zipfile.ZipFile(archive) as z:
        dlls = [z.read(n) for n in z.namelist() if n.lower().endswith('/vulkan-1.dll')]
        matches = [data for data in dlls if machine(data) == expected_machine]
        licenses = [z.read(n) for n in z.namelist() if n.lower().endswith('/vulkanrt-license.txt')]
    if len(matches) != 1 or len(licenses) != 1:
        raise ValueError('archive must contain one matching loader and one runtime license')
    return matches[0], licenses[0]


def check(destination):
    manifest = json.loads((destination / 'manifest.json').read_text())
    for target, (_, expected_machine) in TARGETS.items():
        record = manifest['loaders'][target]
        dll = (destination / target / 'vulkan-1.dll').read_bytes()
        license_text = (destination / target / 'VulkanRT-License.txt').read_bytes()
        if machine(dll) != expected_machine or sha256(dll) != record['dll_sha256']:
            raise ValueError(f'{target}: DLL architecture or checksum mismatch')
        if sha256(license_text) != record['license_sha256']:
            raise ValueError(f'{target}: license checksum mismatch')
        print(f"{target}: Vulkan {record['version']}, PE {expected_machine:#06x}, verified")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--destination', type=Path, default=Path(__file__).resolve().parents[1] / 'third_party/vulkan')
    parser.add_argument('--cache-dir', type=Path, help='Reuse verified component ZIP downloads')
    parser.add_argument('--catalogue', type=Path, help='Use a previously downloaded official catalogue')
    parser.add_argument('--check', action='store_true', help='Verify vendored files without network access')
    args = parser.parse_args()
    if args.check:
        check(args.destination)
        return
    catalogue = json.loads(args.catalogue.read_bytes() if args.catalogue else fetch(CATALOGUE))
    manifest = {'catalogue_url': CATALOGUE, 'updated_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'loaders': {}}
    # Download and validate BOTH architectures before changing any vendored files.
    with tempfile.TemporaryDirectory(prefix='dogwood-vulkan-') as temp:
        staged = Path(temp)
        for target, (platform, expected_machine) in TARGETS.items():
            version, record = latest_runtime(catalogue, platform)
            filename = record['file_name']
            url = f'https://sdk.lunarg.com/sdk/download/{version}/{platform}/{filename}'
            archive = (args.cache_dir or staged) / filename
            archive.parent.mkdir(parents=True, exist_ok=True)
            if not archive.exists():
                print(f'Downloading {url}', flush=True)
                archive.write_bytes(fetch(url))
            digest = sha256(archive.read_bytes())
            if digest != record['sha']:
                raise ValueError(f'{filename}: published SHA-256 mismatch; remove the cached file and retry')
            dll, license_text = unpack(archive, expected_machine)
            directory = staged / target
            directory.mkdir()
            (directory / 'vulkan-1.dll').write_bytes(dll)
            (directory / 'VulkanRT-License.txt').write_bytes(license_text)
            manifest['loaders'][target] = {
                'version': version, 'url': url, 'archive_sha256': digest,
                'dll_sha256': sha256(dll), 'license_sha256': sha256(license_text),
                'pe_machine': f'{expected_machine:#06x}',
            }
        for target in TARGETS:
            directory = args.destination / target
            directory.mkdir(parents=True, exist_ok=True)
            for name in ('vulkan-1.dll', 'VulkanRT-License.txt'):
                (directory / name).write_bytes((staged / target / name).read_bytes())
        (args.destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    check(args.destination)


if __name__ == '__main__':
    main()

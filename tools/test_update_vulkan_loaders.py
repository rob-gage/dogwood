import io
import struct
import unittest
import zipfile

import update_vulkan_loaders as updater


def pe(machine):
    data = bytearray(72)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 0x3c, 64)
    data[64:68] = b'PE\0\0'
    struct.pack_into('<H', data, 68, machine)
    return bytes(data)


class LoaderUpdateTests(unittest.TestCase):
    def test_latest_runtime_skips_newer_sdk_only_and_private_releases(self):
        def release(version, acl='public', runtime=True):
            return {'acl': acl, 'files': [{'file_name': f'VulkanRT-X64-{version}-Components.zip' if runtime else 'sdk.exe', 'acl': acl, 'uploaded': True}]}
        catalogue = {'windows': {'1.4.9.0': release('1.4.9.0'), '1.4.10.0': release('1.4.10.0'),
                                 '1.4.11.0': release('1.4.11.0', runtime=False), '1.4.12.0': release('1.4.12.0', acl='private')}}
        self.assertEqual(updater.latest_runtime(catalogue, 'windows')[0], '1.4.10.0')

    def test_extracts_correct_architecture_without_extracting_archive_paths(self):
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, 'w') as z:
            z.writestr('runtime/x86/vulkan-1.dll', pe(0x14c))
            z.writestr('runtime/x64/vulkan-1.dll', pe(0x8664))
            z.writestr('runtime/VulkanRT-License.txt', b'license')
        archive.seek(0)
        self.assertEqual(updater.unpack(archive, 0x8664), (pe(0x8664), b'license'))
        archive.seek(0)
        with self.assertRaises(ValueError):
            updater.unpack(archive, 0xaa64)

    def test_rejects_invalid_pe(self):
        for data in [b'', b'MZ' + bytes(62), b'not a dll']:
            with self.assertRaises(ValueError):
                updater.machine(data)


if __name__ == '__main__':
    unittest.main()

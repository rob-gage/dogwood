# Bundled DirectX Shader Compiler

Official Microsoft DXC v1.8.2505.1, extracted without modification from the
checksum-pinned release archive recorded in manifest.json. Only dxcompiler.dll
for x64 and ARM64 is shipped. This version uses its internal open source validator;
no separate dxil.dll is needed. ReleaseNotes.md maps the binary to LICENSE-LLVM.txt.
The CLI embeds the compiler, license notices, release notes and manifest.

Run `python3 tools/update_dxc.py --check` for offline checksum/architecture checks.
Run `python3 tools/update_dxc.py` to restore the pinned binaries from Microsoft.

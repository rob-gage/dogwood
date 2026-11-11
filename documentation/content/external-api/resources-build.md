+++
title = "Resources and distributions"
description = "Declarative catalogues, indexed resources, and portable native builds."
weight = 125
[extra]
sources = ["engine_resources", "cli/build.rs", "cli/src/commands/distribute.rs", "cli/src/commands/windows_dxc.rs", "tools/update_dxc.py", "third_party/dxc", "tests/fixtures/resource-game", "engine_host/src/acceleration/performance.rs", "engine_host/build.rs", "template/assets/.resources", "Cargo.toml"]
+++

Install the CLI from an engine checkout with `cargo install --path cli --locked`.
Run builds in a game package with `Cargo.toml` and `assets/.resources`:

```sh
dogwood build --target linux-x64 --release
dogwood build --target windows-x64 --release
dogwood build --target windows-arm64 --release
dogwood build --target linux-x64 --debug
# On a Mac with Xcode Command Line Tools:
dogwood build --target macos-arm64 --release
dogwood build --target macos-x64 --release
```

Release is the default. Target defaults to the host; `linux`, `windows`, `macos`, and
the corresponding Rust triples are aliases. `macos` selects Apple Silicon.
`--path` selects the game directory. With `--output DIR`, final output is
`DIR/<target>/<name>/` and Cargo build files live in `DIR/intermediate/`.
Relative output paths resolve against the game directory. Without `--output`,
final output remains `dist/<target>/<name>/` in the game directory and Cargo uses
its metadata target directory.
A failed compilation preserves an existing distribution. Packaging stages under
Cargo's target directory; target and dist must share a filesystem for final rename.
Concurrent builds of the same game and target are not supported.

Optional `dogwood.toml`:

```toml
name = "MyGame"
binary = "my-game"
bundle_identifier = "games.example.mygame"
# Optional Linux libc baseline, requires cargo-zigbuild and Zig:
linux_glibc = "2.28"
```

Cargo supplies the default binary, version, and target directory. Set `binary`
only when Cargo's default-run or single binary is insufficient. Distribution names
use ASCII letters, digits, hyphens, and underscores. No editor is built implicitly.
The generated template includes an optimized release profile (thin LTO, one codegen
unit, opt-level 3, no debug information); unwinding remains enabled. No host-specific
CPU tuning is applied.

## Catalogue

`assets/.resources` is TOML, parsed using Serde and `toml`. Example:

```toml
[package]
target_size_mib = 64
default_compression = "auto"
compression_level = 3

[types.text]
compression = "zstd"

[[rules]]
pattern = "sprites/**/*.png"
type = "texture"
compression = "none"

[[assets]]
id = "player.idle"
path = "sprites/player/idle.png"
type = "texture"

[[assets]]
id = "debug.grid"
path = "debug/grid.png"
dev_only = true

[[exclude]]
pattern = "source-art/**"
```

Types are descriptive strings: conventional values are `texture`, `audio`, `music`,
`font`, `shader`, `text`, `data`, and `binary`. No implicit image/audio transcoding
occurs. Globbed IDs are normalized relative paths including extension. Later matching
rules replace earlier rule policies. Explicit entries replace the rule entry for their
path and inherit its type/compression/level unless overridden. Explicit IDs must be
unique, including collisions with generated IDs. An explicit asset may not be excluded.
Missing explicit sources, invalid globs, unknown configuration keys, invalid levels,
absolute/traversing paths, and symlinks escaping the root are errors. Unmatched files
are outside the catalogue. Explicitly catalogue required assets so deletion fails the
build rather than merely removing a glob match. Release removes `dev_only` entries.

Compression is `none`, `zstd`, or `auto`; levels are -7 through 22 (default 3).
Per-asset/rule policy overrides per-type policy, which overrides the package default.
Auto bypasses PNG/JPEG/Ogg/Opus/ZIP and retains Zstandard only if it saves more than
5% plus 32 bytes. Music should normally use `none` for bounded streaming.

## Runtime

Use `engine::resources::Resources` (also available in the standalone
`engine_resources` crate). Initialize once and keep it alive:

```rust
let resources = engine::resources::Resources::discover("assets")?;
let image_bytes = resources.load("player.idle")?;
let stream = resources.open("music/theme.ogg")?;
```

`discover` checks beside the executable, then the native app's `Contents/Resources`.
It uses the explicit development root only with debug assertions enabled.
`DOGWOOD_ASSET_ROOT` explicitly selects a loose root even in release; `dogwood run`
sets it to the game source assets to preserve release-mode development workflows. Games
should pass an absolute source root derived from their own Cargo manifest for
working-directory-independent development. `loose` and `packaged` explicitly select
backends. `ids` enumerates the in-memory catalogue. `open` returns an owned `Read +
Seek + Send` bounded reader; seeks and reads cannot enter another asset. Uncompressed
readers use independent file handles, allowing concurrent decoders without shared seek
positions. Compressed readers decompress only the selected asset into memory.
`Audio::prepare_music_reader` accepts these readers for background Vorbis decoding.

Debug distributions contain only catalogued loose files under `assets/`, with a
flattened `.resources` preserving effective IDs/types. Source development reads loose
files directly without generating packages. File bytes can change between reads;
adding/removing IDs requires reopening the catalogue. A game's own decoded caches
may require reload/restart.

## Container layout

Release has `resources/00000000.resource`, `00000001.resource`, etc. Container zero
starts with the eight bytes `DOGWOOD\0`, a little-endian u64 index length, then a
fixed-integer bincode index. Entries contain ID/type, container u32, offset, stored
and original lengths, and a compression boolean. Payload follows the index; later
containers contain payload only. This format has no version field. The reader checks
index length (maximum 64 MiB), duplicates, overlaps, range overflow, truncation, and
decompressed length. It is not an authenticated format; distribute trusted packages.

The packer sorts IDs and packs complete assets sequentially, counting index overhead.
An oversized asset gets its own container; a large index can occupy container zero
alone. Index lookup is loaded once into a BTreeMap; each load seeks directly to one
known range. Source paths and timestamps are absent. Identical catalogues/source
bytes and the same Zstandard implementation produce identical bytes across target
OSes; changes to the compressor dependency can change bytes. Packaging currently
runs on every distribution build; Cargo retains its normal compilation cache.

## Platform prerequisites

Distribution builds query `RUSTC` (or `rustc` on PATH) for its target library directory and
require its `std` and `core` libraries for the selected target. Native distro Rust
installations work without rustup. For missing cross-target libraries, install the
compiler's target packages or use `rustup target add TARGET` with a rustup toolchain.

Linux uses Vulkan exclusively, system `libvulkan.so.1` and the installed GPU driver.
The game also needs ALSA (`libasound.so.2`), glibc, libgcc, and the desktop libraries
loaded by winit (Wayland/X11). Do not copy a host Vulkan loader or vendor driver.
Set `linux_glibc = "2.28"` to use cargo-zigbuild and Zig for an older libc baseline
(`cargo install cargo-zigbuild --locked`; install Zig 0.14.1 on PATH). The CLI uses
Zig for the linker and native C dependencies and permits same-host pkg-config library
interfaces. Host shared libraries are not copied. Otherwise build on the oldest
supported Linux baseline for glibc compatibility: compiling on a
new distribution does not produce an old-glibc-compatible executable. Inspect with
`file`, `ldd`, and `readelf --version-info`.

On Linux, Windows builds use `cargo-xwin`, not bare Cargo cross-linking:

```sh
cargo install cargo-xwin --locked
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
# Fedora: sudo dnf install clang lld llvm
# Debian/Ubuntu: sudo apt install clang lld llvm
```

`clang-cl`, `lld-link`, and `llvm-lib` must be on PATH. cargo-xwin downloads the
Microsoft CRT/SDK and requires their license acceptance. MSVC builds request a static
CRT, avoiding VC runtime DLL deployment. Native Windows uses Cargo with Visual Studio
Build Tools (Desktop development with C++) and the MSVC Rust target. Inspect imports
with `dumpbin /dependents` or `llvm-objdump -p`.

Windows distributions support x64 (`windows-x64`, `x86_64-pc-windows-msvc`)
and ARM64 (`windows-arm64`, `aarch64-pc-windows-msvc`). They use DirectX 12 only.
Each distribution includes the matching official Microsoft DXC 1.8.2505.1
`dxcompiler.dll` beside its executable, plus licenses, release notes and provenance
under `licenses/dxc/`. The CLI embeds these files at compile time. Windows 10/11
include DirectX 12; a capable adapter and driver are still required. No Vulkan
loader or driver is used by the Windows backend.

The host selects DXC by absolute executable-relative path, with no FXC fallback.
For ordinary Cargo builds, `engine_host/build.rs` stages DXC beside binaries and
test executables. This DXC release uses its internal open source validator and
requires no separate `dxil.dll`. The release notes map the binaries to the bundled
LLVM license. Run `python3 tools/update_dxc.py --check` for offline checksum and
architecture verification, or omit `--check` to restore the pinned official files.
Reinstall the CLI after changes to bundled compilers.

macOS requires a Mac, Xcode Command Line Tools (`xcode-select --install`), and
`rustup target add aarch64-apple-darwin x86_64-apple-darwin`. Native Metal is used.
The bundle is `<name>.app/Contents/{Info.plist,MacOS/<name>,Resources/resources/}`
(or `Resources/assets` in debug). Metadata includes bundle ID/version, high-resolution
support, games category and [LSSupportsGameMode](https://developer.apple.com/documentation/bundleresources/information-property-list/lssupportsgamemode).
Apple controls Game Mode eligibility and activation. Signing, notarization, custom
icons, and universal binaries are separate release steps. Native Mac compilation,
GPU limits, fullscreen, and runtime behavior require testing on Apple hardware.

## Shaders and performance

The CLI embeds the engine WGSL corpus, validates assembled modules using Naga 30,
and checks SPIR-V translation as a portable structural check for Linux/Windows, or MSL 2.4 translation for Mac. Windows HLSL/DXIL compilation happens through DXC at runtime.
Game WGSL is validated too; `shade(input: ShadeInput)` fragments receive Dogwood's
lighting/shaded prelude. Fluid material offsets/alignment use representative legal
values for build validation; runtime still substitutes actual material/device values.
Naga checks do not prove adapter capabilities or host binding layouts; runtime wgpu
validation remains necessary. Engine test reference fragments are excluded from the
runtime corpus and checked by their respective tests.

The renderer still consumes WGSL through safe wgpu APIs; wgpu/Naga creates native
SPIR-V on Linux, HLSL/DXIL through DXC on Windows, or MSL on macOS. No unused `.metallib`, unsafe SPIR-V passthrough, or driver-specific
pipeline cache files are shipped. Existing pass pipelines and bind groups are reused;
game-supplied shaded pipelines remain lazily cached on first use. Intentional CPU
readbacks and simulation scheduling are unchanged.

All platforms request a high-performance adapter and check actual required limits.
Windows uses only DX12; Linux uses only Vulkan; Mac uses only Metal. Windows
explicitly disables shader debug compilation so DXC optimizations remain enabled
even in debug Rust builds. Release Windows builds clear instance validation flags. Existing VSync/window controls remain.
`DOGWOOD_PROCESS_PRIORITY=above-normal` opts into Windows above-normal priority or
Linux nice -5, best effort with a diagnostic on refusal. No root/admin dependency,
realtime scheduling, or system power-plan changes are introduced. Windows uses the wgpu high-performance adapter preference for GPU selection. macOS uses normal interactive
window behavior and bundle Game Mode metadata; no custom thread QoS is imposed.

## Verification

```sh
cargo test -p engine_resources -p cli -p engine_audio
cargo test --workspace --lib
dogwood build --path tests/fixtures/resource-game --target linux-x64 --release
(cd /tmp && /absolute/path/to/ResourceGame/ResourceGame)
dogwood build --path tests/fixtures/resource-game --target windows-x64 --release
file tests/fixtures/resource-game/dist/windows-x64/ResourceGame/ResourceGame.exe
```

The fixture is a standalone minimal resource consumer, with no game-specific package
logic. On Windows, run `dogwood build --target windows-x64 --release`, inspect
`dumpbin /dependents`, change to a different directory, and launch the distributed
`.exe`. On Mac, build both supported targets, inspect `plutil -lint .../Info.plist`
and `otool -L .../Contents/MacOS/<name>`, then use `open /absolute/path/<name>.app`.
Test actual gameplay and audio on each platform; cross-compilation is not runtime
verification. The optional authored-audio engine tests require
`DOGWOOD_TEST_MUSIC_DIR` pointing at their expected soundtrack fixture.

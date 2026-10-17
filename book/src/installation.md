# Installation

## Desktop app and command line

Download them from the [releases page](https://github.com/VitalyVorobyev/etendue/releases) (once a
version is released; until then, build from source):

| Platform | Desktop app | Command line |
|---|---|---|
| macOS (Apple silicon) | `.dmg` | `etendue-cli-<version>-aarch64-apple-darwin.tar.gz` |
| Windows (x64) | `.msi`, or the `-setup.exe` installer | `etendue-cli-<version>-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `.AppImage`, or the `.deb` package | `etendue-cli-<version>-x86_64-unknown-linux-gnu.tar.gz` |

The command-line archive holds the `etendue` binary (put it on your `PATH`) and `etendue.toml`.
It has no robot library: to render the examples' robots, use the desktop app or a source checkout
with the robot meshes built (below).

The builds are not signed yet:
- **macOS**: the first time, right-click the app and choose Open, or run
  `xattr -dr com.apple.quarantine "/Applications/etendue studio.app"`.
- **Windows**: SmartScreen warns about an unknown publisher; choose "More info", then "Run anyway".

The desktop app carries its examples and robot library. It writes datasets under
`Documents/etendue` by default.

## Blender

Rendering needs [Blender](https://www.blender.org/download/) **5.1.1**, the version etendue's
renders are validated with. etendue looks for it, in order, in:

1. `--blender <path>` (command line);
2. the `ETENDUE_BLENDER` environment variable;
3. `/Applications/Blender.app/Contents/MacOS/Blender` on macOS, `blender` on the `PATH`
   elsewhere.

The expected version is pinned in an `etendue.toml`: the desktop app carries one, the command-line
archive has one next to the binary, and for your own scenes etendue uses the nearest
`etendue.toml` above the scene, then above the working directory. With a pin, another Blender
version is refused unless you pass `--allow-blender-version`; with no `etendue.toml` found,
etendue renders with whatever Blender it finds and warns that it is unpinned. Keep the archive's
`etendue.toml` in your project folder (or above it) to stay pinned.

Everything except rendering — the studio, validation, baking, ground truth — works without
Blender.

## From source

You need [Rust](https://rustup.rs) (the toolchain is pinned in `rust-toolchain.toml`) and, for the
studio, [bun](https://bun.sh) and [wasm-pack](https://rustwasm.github.io/wasm-pack/). etendue
builds against [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs) checked out
next to it:

```bash
git clone https://github.com/VitalyVorobyev/calibration-rs
git -C calibration-rs checkout v0.9.0
git clone https://github.com/VitalyVorobyev/etendue
cd etendue
cargo install --locked --path crates/etendue-cli   # the `etendue` command
```

The studio, as a desktop app or in a browser:

```bash
uv run --locked --project tools/robot-assets tools/robot-assets/build.py   # robot meshes (needs uv)
node crates/etendue-wasm/scripts/build-npm.mjs
(cd web && bun install)
(cd web/apps/studio && bun run desktop:build)      # a desktop bundle in src-tauri/target/release/bundle
(cd web && bun run dev)                            # or: the studio in a browser, http://localhost:5178
```

The robot meshes are not in the repository (they are rebuilt from the robots' upstream sources);
rendering a scene with a robot needs them, and so does the studio's 3D view.

In a browser the studio can open and play scenes but not generate datasets: that needs the
desktop app (or the command line).

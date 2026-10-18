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

### Prerequisites

- [Rust](https://rustup.rs): `rustup` installs the pinned toolchain (`rust-toolchain.toml`,
  with the wasm32 target) on the first build.
- For the studio: [Node.js](https://nodejs.org), [bun](https://bun.sh) and
  [wasm-pack](https://rustwasm.github.io/wasm-pack/).
- For the robot meshes: [uv](https://docs.astral.sh/uv/) ≥ 0.13.
- On Linux, the desktop app's system libraries:
  `sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
  macOS needs the Xcode command-line tools; Windows needs WebView2 (preinstalled on Windows 11).

etendue builds against [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs) checked
out **next to it** (same parent folder):

```bash
git clone https://github.com/VitalyVorobyev/calibration-rs
git -C calibration-rs checkout v0.9.0
git clone https://github.com/VitalyVorobyev/etendue
cd etendue
```

All commands below run from the `etendue` folder.

### The command line

```bash
cargo install --locked --path crates/etendue-cli   # the `etendue` command, on your PATH
```

### The desktop app

Once per checkout (and again after pulling changes to `crates/etendue-wasm` or the robots):

```bash
uv run --locked --project tools/robot-assets tools/robot-assets/build.py   # robot meshes
node crates/etendue-wasm/scripts/build-npm.mjs                             # the studio's wasm kernel
(cd web && bun install)
```

Then either run it from the checkout, or build an app:

```bash
(cd web/apps/studio && bun run desktop:dev)        # run: opens the app window, reloads on edits
(cd web/apps/studio && bun run desktop:build --bundles app)   # build: the app only (macOS)
(cd web/apps/studio && bun run desktop:build)      # build: every installer for this platform
```

- **`desktop:dev`** compiles the shell (the first time takes a few minutes), starts the frontend
  on `http://localhost:5178`, and opens the window. It reads the examples and robots from the
  checkout and writes datasets under `Documents/etendue`. Leave it running; close the window or
  press Ctrl-C to stop.
- **`desktop:build`** writes to `web/apps/studio/src-tauri/target/release/bundle/`: on macOS
  `macos/etendue studio.app` (copy it to `/Applications`) and, without `--bundles app`, a `.dmg`;
  on Windows an `.msi` and a `-setup.exe`; on Linux an `.AppImage` and a `.deb`. The bundle
  carries the examples, the robot library and `etendue.toml`.

### In a browser

```bash
(cd web && bun run dev)                            # http://localhost:5178
```

After the same once-per-checkout steps. In a browser the studio opens, edits and plays scenes,
but cannot generate datasets: that needs the desktop app or the command line.

The robot meshes are not in the repository (they are rebuilt from the robots' upstream sources);
the studio's 3D view and rendering a scene with a robot need them.

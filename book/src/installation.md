# Installation

etendue comes as two downloads: **etendue studio**, the desktop app, and **`etendue`**, the
command line. Both are on the
[latest release](https://github.com/VitalyVorobyev/etendue/releases/latest). To render images you
also need [Blender 5.2](#blender).

## Downloads

The files of version 0.2.0:

| Platform | Desktop app | Command line |
|---|---|---|
| macOS (Apple silicon) | `etendue-studio-0.2.0-aarch64-apple-darwin.dmg` | `etendue-cli-0.2.0-aarch64-apple-darwin.tar.gz` |
| Windows (x64) | `etendue-studio-0.2.0-x86_64-pc-windows-msvc.msi`, or the installer `etendue-studio-0.2.0-x86_64-pc-windows-msvc-setup.exe` | `etendue-cli-0.2.0-x86_64-pc-windows-msvc.zip` |
| Linux (x64) | `etendue-studio-0.2.0-x86_64-unknown-linux-gnu.AppImage`, or the Debian package `etendue-studio-0.2.0-x86_64-unknown-linux-gnu.deb` | `etendue-cli-0.2.0-x86_64-unknown-linux-gnu.tar.gz` |

Intel Macs and Linux on ARM have no prebuilt files: [build from source](#from-source).

### Check the download

The release also has `SHA256SUMS.txt`, the SHA-256 checksum of every file. In the folder you
downloaded to:

```bash
shasum -a 256 -c --ignore-missing SHA256SUMS.txt     # macOS
sha256sum -c --ignore-missing SHA256SUMS.txt         # Linux
```

Each file you downloaded should print `OK`. On Windows, in PowerShell, compare the output of

```powershell
Get-FileHash .\etendue-studio-0.2.0-x86_64-pc-windows-msvc.msi -Algorithm SHA256
```

with the file's line in `SHA256SUMS.txt`.

### The desktop app

- **macOS**: open the `.dmg` and drag **etendue studio** to Applications.
- **Windows**: run the `.msi` or the `-setup.exe`.
- **Linux**: make the AppImage executable and run it
  (`chmod +x etendue-studio-0.2.0-x86_64-unknown-linux-gnu.AppImage`), or install the package
  (`sudo apt install ./etendue-studio-0.2.0-x86_64-unknown-linux-gnu.deb`).

The app carries the examples, the robot library and the Blender version pin. It writes datasets
under `Documents/etendue` unless you open a project, whose datasets go in its own `datasets/`
folder (see [Projects and the library](library.md)).

**The builds are not signed yet,** so the system warns the first time:
- **macOS** (Gatekeeper): right-click the app in Applications and choose **Open**, then **Open**
  again. Or remove the quarantine flag in a terminal:
  `xattr -dr com.apple.quarantine "/Applications/etendue studio.app"`.
- **Windows** (SmartScreen): choose **More info**, then **Run anyway**.

### The command line

Unpack the archive. It holds the `etendue` binary (`etendue.exe` on Windows), the licence,
`etendue.toml` (which pins the Blender version), the `examples/` and the robot library
(`assets/robots/`, meshes included). Put the binary on your `PATH`, and keep `etendue.toml` in or
above the folders you render from (see [Blender](#blender)).

```bash
tar -xzf etendue-cli-0.2.0-aarch64-apple-darwin.tar.gz
cd etendue-cli-0.2.0-aarch64-apple-darwin
./etendue validate examples/eye_in_hand_ur5e/scene.json examples/eye_in_hand_ur5e/scenario.json
```

A scene names its robot by a path to the robot's `robot.json`, relative to the scene, so the
examples work from the unpacked folder as they are. Your own scenes can point at the same
`assets/robots/` (see [Robots](robots.md)).

## Blender

Rendering needs [Blender](https://www.blender.org/download/) **5.2** (any 5.2.x). etendue's
renders are validated with this series; 5.2.2 is the version tested. Everything else — the
studio, validation, ground truth, detection on existing images, calibration — works without
Blender.

etendue looks for Blender, in order:

1. the path given with `--blender <path>` (command line only);
2. the `ETENDUE_BLENDER` environment variable;
3. `/Applications/Blender.app/Contents/MacOS/Blender` on macOS, `blender` on the `PATH` on
   Windows and Linux.

The studio uses the same search without the first step. On Windows, add Blender's folder (for
example `C:\Program Files\Blender Foundation\Blender 5.2`) to the `PATH`, or set
`ETENDUE_BLENDER` to `blender.exe`.

**The version pin.** An `etendue.toml` file pins the Blender version:

```toml
[blender]
version = "5.2"     # "5.2" accepts every 5.2.x; "5.2.2" accepts only that one
```

The desktop app carries one, and the command-line archive has one next to the binary. For your
own scenes, etendue uses the nearest `etendue.toml` in or above the scene's folder, then in or
above the working directory. With a pin, another Blender version is refused unless you pass
`--allow-blender-version`. With no `etendue.toml` found, etendue renders with whatever Blender it
finds and warns that the version is unpinned. See [Troubleshooting](troubleshooting.md#blender).

## From source

Build from source for an unsupported platform, or to work on etendue itself.

### Prerequisites

- [Rust](https://rustup.rs). `rustup` installs the toolchain the repository pins (with the
  WebAssembly target) on the first build.
- For the studio: [Node.js](https://nodejs.org), [bun](https://bun.sh) and
  [wasm-pack](https://rustwasm.github.io/wasm-pack/).
- For the robot meshes: [uv](https://docs.astral.sh/uv/) 0.13 or later.
- On Linux, the desktop app's system libraries:
  `sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.
  macOS needs the Xcode command-line tools; Windows needs WebView2 (preinstalled on Windows 11).

etendue builds against [calibration-rs](https://github.com/VitalyVorobyev/calibration-rs), checked
out **next to it** (in the same parent folder):

```bash
git clone https://github.com/VitalyVorobyev/calibration-rs
git -C calibration-rs checkout v0.9.0
git clone https://github.com/VitalyVorobyev/etendue
cd etendue
```

All the commands below run from the `etendue` folder.

### The command line

```bash
cargo install --locked --path crates/etendue-cli   # installs `etendue` on your PATH
```

### The robot meshes

The meshes are not in the repository. They are rebuilt from each robot's upstream description:

```bash
uv run --locked --project tools/robot-assets tools/robot-assets/build.py
```

`cargo app` and `cargo app-release` (below) do this for you when the meshes are missing.

### The desktop app

```bash
cargo app                          # build and run the app from the checkout
cargo app-release                  # build the release app and its installers
cargo app-release --bundles app    # macOS: the .app alone
```

Both first prepare what the app needs: the robot meshes if they are missing, the app's
WebAssembly kernel, and its web packages. Any other argument goes to the Tauri command line.

- **`cargo app`** compiles the app (the first time takes a few minutes), serves its interface on
  `http://localhost:5178` and opens the window. It reads the examples and robots from the
  checkout and writes datasets under `Documents/etendue`. Close the window or press Ctrl-C to
  stop.
- **`cargo app-release`** writes to `web/apps/studio/src-tauri/target/release/bundle/`: on macOS
  `macos/etendue studio.app` (copy it to `/Applications`) and a `.dmg`; on Windows an `.msi` and
  a `-setup.exe`; on Linux an `.AppImage` and a `.deb`.

### In a browser

After `cargo app` or `cargo app-release` has prepared the checkout once:

```bash
(cd web && bun run dev)            # http://localhost:5178
```

In a browser the studio opens, edits and plays scenes, designs heads and rigs, and previews what
a calibration observes. It cannot render, detect or calibrate: those need the desktop app or the
command line. Opening a project folder needs a browser that can open folders (Chrome or Edge).

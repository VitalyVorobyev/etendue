# Troubleshooting

## Installing and starting

**macOS says the app "cannot be opened" or "is damaged".** The builds are not signed yet. Right-click
**etendue studio** in Applications and choose **Open**, then **Open** again, or remove the
quarantine flag:

```bash
xattr -dr com.apple.quarantine "/Applications/etendue studio.app"
```

The same applies to the `etendue` command from the archive:
`xattr -d com.apple.quarantine /usr/local/bin/etendue`.

**Windows SmartScreen warns about an unknown publisher.** Choose **More info**, then **Run
anyway**.

**A download looks wrong.** Check it against `SHA256SUMS.txt` from the same release
([Installation](installation.md#check-the-download)).

**`reading robot manifest …/robot.json: No such file or directory`.** A scene names its robot by
a path relative to the scene. The examples expect the repository's `assets/robots/` two folders
up, so run them from a clone of the repository. The command-line archive has no robot library.

## Blender

**"Blender not found"** (the studio), or **`running … (set --blender or $ETENDUE_BLENDER)`** (the
command line). etendue looks for Blender with `--blender`, then `ETENDUE_BLENDER`, then
`/Applications/Blender.app` on macOS or `blender` on the `PATH` elsewhere:
- install [Blender 5.2](https://www.blender.org/download/) in the default place;
- on Windows and Linux, put Blender's folder on the `PATH`, or set `ETENDUE_BLENDER` to the
  executable;
- on the command line, `--blender <path>` names it directly.

An app started from the Dock or the Start menu may not see variables set in a shell profile. Set
`ETENDUE_BLENDER` system-wide, or start the app from a terminal where it is set.

**`Blender 4.5.3 at … but …/etendue.toml pins 5.2; install it, point --blender at it, or pass
--allow-blender-version`.** etendue renders only with the Blender version
its `etendue.toml` pins, because renders are validated with it. Install Blender 5.2 (any 5.2.x),
and point etendue at it if it is not the one found first. On the command line,
`--allow-blender-version` renders with another version anyway, with a warning; the images may
then differ from validated ones. The studio has no such override: it shows the mismatch next to
the Blender status, and rendering needs the pinned version.

**`warning: no etendue.toml Blender pin found`.** No `etendue.toml` is in or above the scene's
folder or the working directory. etendue renders with whatever Blender it finds. Keep the
`etendue.toml` from the command-line archive in or above your project folder to stay pinned.

**`Blender failed (…)`.** The last lines of Blender's own log follow the message. Common causes
are a GPU driver problem (try `--cpu`, or **Render on the CPU** in the studio) or running out of
memory at a high supersampling (lower `--supersample`, or render fewer cameras with `--camera`).

**Rendering is slow.** The time grows with the square of `--supersample` and linearly with
`--samples`. Check images with `--supersample 1` first, and use 4 only for the final dataset. As
a guide, the `laser_ur5e` example (48 images and 48 laser images of 1280 × 1024) took about 11
minutes at supersampling 4 and 16 samples on an Apple M4 Pro.

## The browser build

In a browser, the studio cannot render, detect or calibrate; those need the desktop app or the
command line. The Calibrate workspace only previews the observations. **Open project…** needs a
browser that can open folders (Chrome or Edge); in others, open files with **Open files…** or by
dropping them on the window, and save with **Download scene**. See
[The studio](studio.md#in-a-browser).

## Robots and motion

**`inverse kinematics failed: robot … residual … after 16 restart(s)`.** No joint configuration
within the limits reaches the pose. Check, in order:
- the pose is in the robot's **base frame** (for the UR5e, `base`, which is turned half a turn
  from `base_link`);
- the translation is in metres and the quaternion is `[qx, qy, qz, qw]`, scalar last;
- the pose is within reach: jog the robot to it in the studio (**Motion › Jog**), which tells you
  when the arm cannot follow;
- a joint limit does not block the way.

For generated poses, `etendue scenario sphere --candidates c.json` writes why each candidate was
dropped (`unreachable`, `near_limit`, `near_singular`, `not_visible`), and the studio colours them
in the 3-D view.

**The arm swings round between two nearby poses, or moves differently after an edit.** Inverse
kinematics found the pose in another configuration (elbow or wrist flipped). The motion report
warns "IK found this pose only from restart …". Press **Resolve IK** in the Program panel, or run
`etendue export … --resolved scenario.json`: every pose move then keeps its joint configuration.
See [Scenes and scenarios](scenes.md#the-scenario).

**Motion warnings.** The Program panel marks a step with ⚠, and `etendue export --report` lists:
- **near a joint limit**: a joint comes within 2° of its limit. Rotate the target pose about the
  tool axis (a different roll), or move the target;
- **near a singularity**: the tool Jacobian's smallest singular value drops below 0.02 (wrist
  axes lining up, or the arm stretched out). Tilt the tool a little, or approach from another
  side;
- **large joint jump**: a straight-line move needs a joint change of more than 0.05 rad between
  waypoints, typically through a singularity. Use a point-to-point move instead, or split the
  line.

Warnings do not stop a scenario; a pose out of reach or a joint outside its limits does.

## Detection

`etendue detect` (and the studio after **Generate**) gives every view a status. When views are
missing:

| Status | Meaning | What to try |
|---|---|---|
| `no_image` | The image was not rendered. | Render that camera (`--camera`, or **Cameras** in the studio). |
| `no_board` | The detector found no target. | Check the image: too dark (raise `--exposure`, `--ambient` or the sensor's `electrons_per_unit`), too bright and clipped, or the target too small or too oblique. |
| `partial` | Too little of the target is seen. A chessboard must be seen whole. | Keep the whole board in view (the pose generator's **seen** at 100 %), or use a self-identifying target (ChArUco, PuzzleBoard). |
| `ambiguous` | The target's orientation is not decided, for example a chessboard whose square colours do not tell its half-turn. | Even out the light, or use a target with markers. |
| `undecoded` | A grid was found, but its markers, circles or code did not identify it. | Use more pixels per square (a closer view, a longer lens), or a higher supersampling. |

**Mislabelled or unmatched points** are reported, never corrected: a detection's label disagrees
with the ground truth, or no true point is near it. A few on extreme views are a detector's
limit; many point to a wrong target description.

**Corners far from the truth.** Use `--supersample 4` and a sensor model (linear images): sRGB
images and coarse renders add error to corner positions ([Rendering](rendering.md)).

**No laser stripe found.** The stripe must stand at least 8 grey levels above the dimmed board
image. Raise the laser's `power_w`, lower `--laser-ambient`, or check that the laser's fan
reaches the target (`fan_length`) and that the camera images it (`camera` on the laser).

## Calibration

| Message | Meaning |
|---|---|
| `the dataset has no robot: there is no hand-eye to calibrate` | `etendue calibrate` calibrates hand-eye cells only. Calibrate `dataset.json` with calibration-rs directly. |
| `--camera does not apply to a laser dataset` | A laser dataset is calibrated as a whole. |
| `--noise and --trials apply to the analytic observations, not to --features` | Detections already carry their own error. |
| `parsing observation noise …: unknown field …` | A misspelt noise field; the message lists the valid ones. |
| `the observations do not match the ground truth's captures and cameras` | `features.json` is from another run of `gt`. Detect again after `gt` and `render`. |
| `closed-loop check (target) failed`, `closed-loop check (target and laser plane) failed` | On exact observations, a quantity is outside its tolerance ([Calibration](calibration.md#the-tolerances)). For a Scheimpflug laser rig this is a known limitation; otherwise please report it with the scene. |

## Logs

- **The command line** prints progress to standard output and warnings and errors to standard
  error. During `render`, Blender's own log lines are passed through.
- **The studio** shows the progress and Blender's log in the Calibrate workspace while it
  generates a dataset, and errors as notifications. Other messages go to the app's standard
  error: start the app from a terminal to see them, for example on macOS
  `"/Applications/etendue studio.app/Contents/MacOS/etendue-studio"`.
- **In a browser**, problems reading a project's files are listed in the browser's developer
  console.

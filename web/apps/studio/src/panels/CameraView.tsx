import type { CameraSpec, EtendueScene, LaserSpec, ObservedView, TargetSpec } from "@etendue/wasm";
import {
  ImageStage,
  MeasureOverlay,
  type MeasurePrimitive,
  StageButton,
  StageReadout,
  StageToolbar,
  type StageView,
  useStage,
} from "@vitavision/stage2d";
import { SensorImage } from "@vitavision/three-react";
import { Grid3x3, Image as ImageIcon } from "lucide-react";
import { type FrameTreeRuntime, matrixFromIso3 } from "@vitavision/three";
import { type Playhead, usePlayhead } from "@vitavision/workbench";
import { useMemo, useState } from "react";
import { Vector3 } from "three";

import type { PoseSource } from "../kernel/posed";
import { targetOutline } from "../scene/targets";

/** `[u0, v0, u1, v1, …]` with `NaN` breaks as segments between consecutive seen points. */
function polyline(uv: Float64Array): number[] {
  const segments: number[] = [];
  for (let j = 0; j + 3 < uv.length; j += 2) {
    if ([uv[j], uv[j + 1], uv[j + 2], uv[j + 3]].every((x) => Number.isFinite(x))) {
      segments.push(uv[j]!, uv[j + 1]!, uv[j + 2]!, uv[j + 3]!);
    }
  }
  return segments;
}

/**
 * What the camera sees of every target at sample `k`: the outline of a planar target, the
 * corners it sees (behind-the-camera, out-of-image and back-facing ones are left out, by the
 * kernel's ground-truth visibility), and the stripe of every laser it images (its own, or a laser
 * linked to no camera). The projection is the kernel's; this only strings points together.
 */
function project(
  session: EtendueScene,
  runtime: PoseSource,
  camera: CameraSpec,
  targets: readonly TargetSpec[],
  lasers: readonly LaserSpec[],
  k: number,
): { primitives: MeasurePrimitive[]; visible: number; stripes: number } {
  const worldSe3Camera = runtime.pose(camera.id, k);
  if (!worldSe3Camera) return { primitives: [], visible: 0, stripes: 0 };
  const primitives: MeasurePrimitive[] = [];
  let visible = 0;
  let stripes = 0;
  const p = new Vector3();
  for (const target of targets) {
    const pose = runtime.pose(target.id, k);
    if (!pose) continue;
    let extent: [number, number] | undefined;
    try {
      extent = session.targetExtent(target.id);
    } catch {
      continue;
    }
    if (extent) {
      const m = matrixFromIso3(pose);
      const line = targetOutline(extent);
      const world = new Float64Array(line.length);
      for (let j = 0; j < line.length; j += 3) {
        p.set(line[j]!, line[j + 1]!, line[j + 2]).applyMatrix4(m);
        world.set([p.x, p.y, p.z], j);
      }
      const uv = session.projectPoints(camera.id, worldSe3Camera, world);
      const segments: number[] = [];
      for (let j = 0; j + 3 < uv.length; j += 2) segments.push(uv[j]!, uv[j + 1]!, uv[j + 2]!, uv[j + 3]!);
      primitives.push({ kind: "segments", points: segments, tone: "signal" });
    }
    let corners: Float64Array;
    try {
      corners = session.projectTarget(camera.id, worldSe3Camera, target.id, pose);
    } catch {
      continue;
    }
    let seen = 0;
    for (let j = 0; j < corners.length; j += 2) {
      const [u, v] = [corners[j]!, corners[j + 1]!];
      if (!Number.isFinite(u)) continue;
      seen++;
      primitives.push({ kind: "point", x: u, y: v, cross: true, tone: "signal" });
    }
    if (seen > 0) visible++;
    for (const laser of lasers) {
      if (laser.camera !== undefined && laser.camera !== camera.id) continue;
      const laserPose = runtime.pose(laser.id, k);
      if (!laserPose) continue;
      try {
        const stripe = session.laserStripe(camera.id, worldSe3Camera, laser.id, laserPose, target.id, pose);
        const segments = polyline(stripe);
        if (segments.length > 0) {
          primitives.push({ kind: "segments", points: segments, tone: "defect" });
          stripes++;
        }
      } catch {
        // A laser the kernel cannot place draws nothing.
      }
    }
  }
  return { primitives, visible, stripes };
}

/**
 * What the calibration sees of a view: each observed point as a dot (an outlier larger, in the
 * defect colour) and the observed stripe's pixels. A true corner without a dot was dropped.
 */
function observedMarks(view: ObservedView): MeasurePrimitive[] {
  const marks: MeasurePrimitive[] = view.laser.map(([x, y]) => ({ kind: "point", x, y, radius: 1.5, tone: "warn" }));
  for (const p of view.points) {
    const [x, y] = p.pixel;
    marks.push(
      p.outlier ? { kind: "point", x, y, radius: 6, tone: "defect" } : { kind: "point", x, y, radius: 4, tone: "warn" },
    );
  }
  return marks;
}

function Overlay({ width, height, primitives }: { width: number; height: number; primitives: MeasurePrimitive[] }) {
  const stage = useStage();
  return (
    <MeasureOverlay nativeWidth={width} nativeHeight={height} primitives={primitives} strokeScale={stage.view.scale} />
  );
}

/**
 * Pixel-centre convention of the web backend: integer coordinates name pixel centres, as
 * `@vitavision/stage2d` draws them. **Provisional** until probe P4-2 measures it (ADR 0004).
 */
const PIXEL_CENTRE = "integer";

/**
 * What one camera sees at the playhead:
 * - the rendered image — the canonical pinhole resampled through the kernel's remap LUT, so
 *   distortion and sensor tilt are the calibrated model's (ADR 0004);
 * - over it, the targets' outlines, the corners the camera sees and its lasers' stripes,
 *   projected analytically by the kernel. Where the two agree the render is registered with the
 *   model.
 *
 * With a generated dataset (the Tauri shell), the toolbar switches the live render for the
 * dataset's Blender image at each capture.
 */
export function CameraView({
  session,
  runtime,
  poses = runtime,
  camera,
  targets,
  lasers = [],
  playhead,
  renderedAt,
  observedAt,
}: {
  session: EtendueScene;
  runtime: FrameTreeRuntime;
  /** Where poses are read: `runtime`, or it with an edit in progress drawn over it. */
  poses?: PoseSource | undefined;
  camera: CameraSpec;
  targets: readonly TargetSpec[];
  /** The scene's lasers; the camera's own stripes are drawn. */
  lasers?: readonly LaserSpec[] | undefined;
  playhead: Playhead;
  /** The dataset image of `camera` at sample `k`, if one was rendered there. */
  renderedAt?: ((camera: string, k: number) => string | null) | undefined;
  /** What the calibration observes of `camera` at sample `k`, if `k` is a capture (P13). */
  observedAt?: ((camera: string, k: number) => ObservedView | null) | undefined;
}) {
  const k = usePlayhead(playhead);
  const [view, setView] = useState<StageView | null>(null);
  const [overlay, setOverlay] = useState(true);
  const [showRendered, setShowRendered] = useState(true);
  const rendered = showRendered ? (renderedAt?.(camera.id, k) ?? null) : null;
  const hasDataset = renderedAt !== undefined && renderedAt(camera.id, k) !== null;
  const remap = useMemo(() => {
    const { canonical, lut } = session.remap(camera.id, { supersample: 1 }, PIXEL_CENTRE);
    const k = canonical.params.intrinsics;
    return {
      canonical: { width: canonical.resolution[0], height: canonical.resolution[1], focalPx: k.fx },
      lut: { width: camera.resolution[0], height: camera.resolution[1], data: lut, pixelCentre: PIXEL_CENTRE },
    } as const;
  }, [session, camera]);
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);
  const { primitives, visible, stripes } = useMemo(
    () => project(session, poses, camera, targets, lasers, k),
    [session, poses, camera, targets, lasers, k],
  );
  const observed = observedAt?.(camera.id, k) ?? null;
  const marks = useMemo(
    () => (observed ? [...primitives, ...observedMarks(observed)] : primitives),
    [primitives, observed],
  );
  const [w, h] = [camera.resolution[0], camera.resolution[1]];
  return (
    <div
      className="flex h-full min-h-0 flex-col"
      data-testid={`camera-view-${camera.id}`}
      data-visible={visible}
      data-stripes={stripes}
      data-observed={observed ? observed.points.length : undefined}
    >
      <ImageStage
        image={{ width: w, height: h }}
        view={view}
        onView={setView}
        onHover={setCursor}
        label={`${camera.id} image`}
        className="min-h-0 flex-1"
        toolbar={
          <StageToolbar>
            <StageButton label="Projected targets" pressed={overlay} onClick={() => setOverlay(!overlay)}>
              <Grid3x3 className="size-4" aria-hidden />
            </StageButton>
            {hasDataset && (
              <StageButton
                label="Dataset image (Blender)"
                pressed={showRendered}
                onClick={() => setShowRendered(!showRendered)}
              >
                <ImageIcon className="size-4" aria-hidden />
              </StageButton>
            )}
          </StageToolbar>
        }
        readout={<StageReadout cursor={cursor} />}
      >
        {rendered !== null ? (
          <img
            src={rendered}
            alt={`${camera.id} dataset image`}
            data-testid={`dataset-image-${camera.id}`}
            className="absolute inset-0 h-full w-full border border-line-strong"
            style={{ imageRendering: "pixelated" }}
            draggable={false}
          />
        ) : (
          <SensorImage
            runtime={runtime}
            frame={camera.id}
            canonical={remap.canonical}
            lut={remap.lut}
            playhead={playhead}
            className="absolute inset-0 h-full w-full border border-line-strong"
            label={`${camera.id} rendered image`}
          />
        )}
        {/* Only what lands on the sensor is drawn. */}
        {overlay && (
          <div className="absolute inset-0 overflow-hidden">
            <Overlay width={w} height={h} primitives={marks} />
          </div>
        )}
      </ImageStage>
    </div>
  );
}

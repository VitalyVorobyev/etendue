import type { CameraSpec, EtendueScene, TargetSpec } from "@etendue/wasm";
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

import { checkerOf, targetPolylines } from "../scene/targets";

interface TargetLines {
  id: string;
  /** Polylines in the target frame; the first `outline` of them are the outline. */
  lines: Float64Array[];
}

/**
 * Project every target's polylines into `camera` at sample `k`: segments between consecutive
 * imaged samples. The projection is the kernel's; this only strings points together.
 */
function project(
  session: EtendueScene,
  runtime: FrameTreeRuntime,
  camera: CameraSpec,
  targets: TargetLines[],
  k: number,
): { primitives: MeasurePrimitive[]; visible: number } {
  const worldSe3Camera = runtime.pose(camera.id, k);
  if (!worldSe3Camera) return { primitives: [], visible: 0 };
  const [w, h] = [camera.resolution[0], camera.resolution[1]];
  const primitives: MeasurePrimitive[] = [];
  let visible = 0;
  const p = new Vector3();
  for (const target of targets) {
    const pose = runtime.pose(target.id, k);
    if (!pose) continue;
    const m = matrixFromIso3(pose);
    let inImage = 0;
    for (const [i, line] of target.lines.entries()) {
      const world = new Float64Array(line.length);
      for (let j = 0; j < line.length; j += 3) {
        p.set(line[j]!, line[j + 1]!, line[j + 2]).applyMatrix4(m);
        world.set([p.x, p.y, p.z], j);
      }
      const uv = session.projectPoints(camera.id, worldSe3Camera, world);
      const border = i === 0 || i === target.lines.length - 1;
      for (let j = 0; j + 3 < uv.length; j += 2) {
        const [u0, v0, u1, v1] = [uv[j]!, uv[j + 1]!, uv[j + 2]!, uv[j + 3]!];
        if (![u0, v0, u1, v1].every(Number.isFinite)) continue;
        if (u0 >= 0 && u0 <= w && v0 >= 0 && v0 <= h) inImage++;
        primitives.push({ kind: "segment", x1: u0, y1: v0, x2: u1, y2: v1, tone: border ? "signal" : "muted" });
      }
    }
    if (inImage > 0) visible++;
  }
  return { primitives, visible };
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
 * - over it, the targets' outlines and square grids projected analytically by the kernel.
 *   Where the two agree the render is registered with the model.
 *
 * With a generated dataset (the Tauri shell), the toolbar switches the live render for the
 * dataset's Blender image at each capture.
 */
export function CameraView({
  session,
  runtime,
  camera,
  targets,
  playhead,
  renderedAt,
}: {
  session: EtendueScene;
  runtime: FrameTreeRuntime;
  camera: CameraSpec;
  targets: readonly TargetSpec[];
  playhead: Playhead;
  /** The dataset image of `camera` at sample `k`, if one was rendered there. */
  renderedAt?: ((camera: string, k: number) => string | null) | undefined;
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
  const lines = useMemo<TargetLines[]>(
    () =>
      targets.flatMap((t) => {
        const extent = session.targetExtent(t.id);
        if (!extent) return [];
        const polylines = targetPolylines(extent, checkerOf(t));
        return [{ id: t.id, lines: polylines }];
      }),
    [session, targets],
  );
  const { primitives, visible } = useMemo(
    () => project(session, runtime, camera, lines, k),
    [session, runtime, camera, lines, k],
  );
  const [w, h] = [camera.resolution[0], camera.resolution[1]];
  return (
    <div className="flex h-full min-h-0 flex-col" data-testid={`camera-view-${camera.id}`} data-visible={visible}>
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
              <StageButton label="Dataset image (Blender)" pressed={showRendered} onClick={() => setShowRendered(!showRendered)}>
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
            <Overlay width={w} height={h} primitives={primitives} />
          </div>
        )}
      </ImageStage>
    </div>
  );
}

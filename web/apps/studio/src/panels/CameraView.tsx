import type { CameraSpec, EtendueScene, TargetSpec } from "@etendue/wasm";
import {
  ImageStage,
  MeasureOverlay,
  type MeasurePrimitive,
  StageReadout,
  StageToolbar,
  type StageView,
  useStage,
} from "@vitavision/stage2d";
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
 * What one camera sees of the targets at the playhead: their outlines and square grids,
 * projected by the kernel through the camera's calibrated model. An analytic preview, not a
 * render (rendering is P3/P4).
 */
export function CameraView({
  session,
  runtime,
  camera,
  targets,
  playhead,
}: {
  session: EtendueScene;
  runtime: FrameTreeRuntime;
  camera: CameraSpec;
  targets: readonly TargetSpec[];
  playhead: Playhead;
}) {
  const k = usePlayhead(playhead);
  const [view, setView] = useState<StageView | null>(null);
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
        toolbar={<StageToolbar />}
        readout={<StageReadout cursor={cursor} />}
      >
        <div className="absolute inset-0 border border-line-strong bg-canvas" />
        {/* Only what lands on the sensor is drawn. */}
        <div className="absolute inset-0 overflow-hidden">
          <Overlay width={w} height={h} primitives={primitives} />
        </div>
      </ImageStage>
    </div>
  );
}

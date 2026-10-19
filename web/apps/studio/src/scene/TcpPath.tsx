/**
 * Where each robot's TCP goes over the program: a polyline through the baked samples, and a dot
 * at every capture. Drawn in the world frame. The path is coloured by program step: steps
 * alternate between two slate tones, a step with a motion warning is in the warning colour, and the
 * step at the playhead in the accent. Pointing at the path marks the nearest sample; a click
 * seeks the playhead there.
 */

import type { BakedScenario, MotionReport } from "@etendue/wasm";
import { useSceneColors } from "@vitavision/three-react";
import type { ThreeEvent } from "@react-three/fiber";
import { useEffect, useMemo, useState } from "react";
import {
  BufferGeometry,
  Color,
  Float32BufferAttribute,
  type Intersection,
  Line,
  LineBasicMaterial,
  type Raycaster,
} from "three";

/** At most this many points per path. */
const MAX_POINTS = 2000;
// Steps alternate between two quiet slate tones, so the accent marks the step at the playhead.
const PATH = new Color().setRGB(0.36, 0.45, 0.55);
const PATH_ALT = new Color().setRGB(0.62, 0.69, 0.77);
const CAPTURE = new Color().setRGB(0.89, 0.34, 0.18);
/** How far from the path a pointer still finds it, as a share of the distance to it. */
const PICK_ANGLE = 0.01;

/** A robot's TCP over the bake. */
export interface TcpPathData {
  /** World positions, flat `[x, y, z, …]`. */
  path: Float32Array;
  /** The sample of each path point. */
  samples: Int32Array;
  /** Their times (s). */
  times: Float32Array;
  /** World positions at the captures. */
  captures: Float32Array;
}

/** The world positions of `frame` over the bake, and at its captures. */
export function tcpPath(baked: BakedScenario, frame: string): TcpPathData | null {
  const f = baked.frames.indexOf(frame);
  if (f < 0) return null;
  const stride = Math.max(1, Math.ceil(baked.samples.length / MAX_POINTS));
  const path: number[] = [];
  const samples: number[] = [];
  const captures: number[] = [];
  baked.samples.forEach((s, k) => {
    const t = s.world_se3_frame[f]!.translation;
    if (k % stride === 0 || k === baked.samples.length - 1) {
      path.push(t[0], t[1], t[2]);
      samples.push(k);
    }
    if (s.capture) captures.push(t[0], t[1], t[2]);
  });
  return {
    path: new Float32Array(path),
    samples: new Int32Array(samples),
    times: new Float32Array(samples.map((k) => baked.samples[k]!.t)),
    captures: new Float32Array(captures),
  };
}

/**
 * The program step running at time `t`: the last that started by then, or -1 before the first.
 * With `moving`, the last robot motion instead (at a capture, the move that led there).
 */
export function stepAt(report: MotionReport | null, t: number, moving = false): number {
  let at = -1;
  for (const s of report?.steps ?? []) {
    if (s.t0 <= t + 1e-9 && (!moving || (s.robot != null && s.duration_s > 0))) at = s.step;
  }
  return at;
}

/** Per path point, the colour of its step (`current` is the step at the playhead). */
export function pathColors(
  times: Float32Array,
  report: MotionReport | null,
  current: number,
  colors: { warn: Color; current: Color },
): Float32Array {
  const out = new Float32Array(times.length * 3);
  const steps = report?.steps ?? [];
  for (let i = 0; i < times.length; i++) {
    const step = stepAt(report, times[i]!);
    const c =
      step === current && step >= 0
        ? colors.current
        : (steps[step]?.warnings?.length ?? 0) > 0
          ? colors.warn
          : step % 2 === 0
            ? PATH
            : PATH_ALT;
    out.set([c.r, c.g, c.b], 3 * i);
  }
  return out;
}

/** A line found within a cone around the pointer's ray, not within a fixed metre. */
function pickable(line: Line): Line {
  line.geometry.computeBoundingSphere();
  const centre = line.geometry.boundingSphere?.center;
  line.raycast = (raycaster: Raycaster, intersects: Intersection[]) => {
    const distance = centre ? raycaster.ray.origin.distanceTo(centre) : 1;
    const before = raycaster.params.Line.threshold;
    raycaster.params.Line.threshold = PICK_ANGLE * distance;
    Line.prototype.raycast.call(line, raycaster, intersects);
    raycaster.params.Line.threshold = before;
  };
  return line;
}

export function TcpPath({
  data,
  report,
  current,
  onSeek,
}: {
  data: TcpPathData;
  report: MotionReport | null;
  /** The program step at the playhead. */
  current: number;
  onSeek?: ((k: number) => void) | undefined;
}) {
  const scene = useSceneColors();
  const [hover, setHover] = useState<number | null>(null);
  const colors = useMemo(
    () => pathColors(data.times, report, current, { warn: new Color(scene.warn), current: new Color(scene.signal) }),
    [data, report, current, scene],
  );
  const line = useMemo(() => {
    const geometry = new BufferGeometry()
      .setAttribute("position", new Float32BufferAttribute(data.path, 3))
      .setAttribute("color", new Float32BufferAttribute(colors, 3));
    return pickable(new Line(geometry, new LineBasicMaterial({ vertexColors: true })));
  }, [data, colors]);
  useEffect(
    () => () => {
      line.geometry.dispose();
      (line.material as LineBasicMaterial).dispose();
    },
    [line],
  );
  const dots = useMemo(
    () => new BufferGeometry().setAttribute("position", new Float32BufferAttribute(data.captures, 3)),
    [data],
  );
  const marker = useMemo(() => {
    if (hover === null) return null;
    return new BufferGeometry().setAttribute(
      "position",
      new Float32BufferAttribute(data.path.slice(3 * hover, 3 * hover + 3), 3),
    );
  }, [data, hover]);
  // The nearest path point of a hit on segment `index` (from its start).
  const nearest = (e: ThreeEvent<PointerEvent | MouseEvent>): number | null => {
    const i = e.index;
    if (i === undefined) return null;
    const p = e.point;
    const at = (j: number) =>
      p.distanceToSquared({ x: data.path[3 * j]!, y: data.path[3 * j + 1]!, z: data.path[3 * j + 2]! });
    const j = Math.min(i + 1, data.samples.length - 1);
    return at(i) <= at(j) ? i : j;
  };
  return (
    <group name="tcp-path">
      <primitive
        object={line}
        onPointerMove={(e: ThreeEvent<PointerEvent>) => {
          e.stopPropagation();
          setHover(nearest(e));
        }}
        onPointerOut={() => setHover(null)}
        onClick={(e: ThreeEvent<MouseEvent>) => {
          e.stopPropagation();
          const i = nearest(e);
          if (i !== null) onSeek?.(data.samples[i]!);
        }}
      />
      <points>
        <primitive object={dots} attach="geometry" />
        <pointsMaterial color={CAPTURE} size={7} sizeAttenuation={false} />
      </points>
      {marker && (
        <points>
          <primitive object={marker} attach="geometry" />
          <pointsMaterial color={scene.signal} size={11} sizeAttenuation={false} />
        </points>
      )}
    </group>
  );
}

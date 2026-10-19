/**
 * Where each robot's TCP goes over the program: a polyline through the baked samples, and a dot
 * at every capture. Drawn in the world frame.
 */

import type { BakedScenario } from "@etendue/wasm";
import { useMemo } from "react";
import { BufferGeometry, Color, Float32BufferAttribute } from "three";

/** At most this many points per path. */
const MAX_POINTS = 2000;
const PATH = new Color().setRGB(0.16, 0.5, 0.73);
const CAPTURE = new Color().setRGB(0.89, 0.34, 0.18);

/** The world positions of `frame` over the bake, and at its captures. */
export function tcpPath(baked: BakedScenario, frame: string): { path: Float32Array; captures: Float32Array } | null {
  const f = baked.frames.indexOf(frame);
  if (f < 0) return null;
  const stride = Math.max(1, Math.ceil(baked.samples.length / MAX_POINTS));
  const path: number[] = [];
  const captures: number[] = [];
  baked.samples.forEach((s, k) => {
    const t = s.world_se3_frame[f]!.translation;
    if (k % stride === 0 || k === baked.samples.length - 1) path.push(t[0], t[1], t[2]);
    if (s.capture) captures.push(t[0], t[1], t[2]);
  });
  return { path: new Float32Array(path), captures: new Float32Array(captures) };
}

export function TcpPath({ path, captures }: { path: Float32Array; captures: Float32Array }) {
  const line = useMemo(
    () => new BufferGeometry().setAttribute("position", new Float32BufferAttribute(path, 3)),
    [path],
  );
  const dots = useMemo(
    () => new BufferGeometry().setAttribute("position", new Float32BufferAttribute(captures, 3)),
    [captures],
  );
  return (
    <group name="tcp-path">
      <line>
        <primitive object={line} attach="geometry" />
        <lineBasicMaterial color={PATH} />
      </line>
      <points>
        <primitive object={dots} attach="geometry" />
        <pointsMaterial color={CAPTURE} size={7} sizeAttenuation={false} />
      </points>
    </group>
  );
}

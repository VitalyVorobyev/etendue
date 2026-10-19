/**
 * The rig in 3D: its cameras' fields, its lasers' fans, the box it is judged over, and the voxels
 * it covers. The rig hangs looking down, its box resting on the floor grid.
 */

import { useThree } from "@react-three/fiber";
import { CameraFrustum, LaserFan, SceneCanvas } from "@vitavision/three-react";
import { useLayoutEffect, useMemo, useRef } from "react";
import { BoxGeometry, Color, type InstancedMesh, Matrix4, MeshLambertMaterial, Vector3 } from "three";

import type { RigAnalysis } from "./analysis";
import { MEASURED, VIEW_COLOURS } from "./SliceMaps";

/** What the voxels show: those seen by at least `views` cameras, or those a pair measures. */
export type VoxelShow = { kind: "views"; min: number } | { kind: "measured" };

function Voxels({ analysis, show }: { analysis: RigAnalysis; show: VoxelShow }) {
  const mesh = useRef<InstancedMesh>(null);
  const invalidate = useThree((s) => s.invalidate);
  const { grid, coverage } = analysis;
  const [nx, ny, nz] = grid.n;
  const d = useMemo(
    () => [0, 1, 2].map((i) => (grid.max[i]! - grid.min[i]!) / grid.n[i]!) as [number, number, number],
    [grid],
  );
  const picked = useMemo(() => {
    const out: number[] = [];
    for (let i = 0; i < coverage.views.length; i++) {
      if (show.kind === "views" ? coverage.views[i]! >= show.min : coverage.lit[i]! >= 1) out.push(i);
    }
    return out;
  }, [coverage, show]);
  const geometry = useMemo(() => new BoxGeometry(d[0] * 0.7, d[1] * 0.7, d[2] * 0.7), [d]);
  const material = useMemo(() => new MeshLambertMaterial({ transparent: true, opacity: 0.55 }), []);
  useLayoutEffect(() => {
    const m = mesh.current;
    if (!m) return;
    const at = new Matrix4();
    const colour = new Color();
    picked.forEach((index, n) => {
      const i = index % nx;
      const j = Math.floor(index / nx) % ny;
      const k = Math.floor(index / (nx * ny));
      at.makeTranslation(
        grid.min[0] + (i + 0.5) * d[0],
        grid.min[1] + (j + 0.5) * d[1],
        grid.min[2] + (k + 0.5) * d[2],
      );
      m.setMatrixAt(n, at);
      colour.set(
        show.kind === "measured" ? MEASURED : VIEW_COLOURS[Math.min(VIEW_COLOURS.length, coverage.views[index]!) - 1]!,
      );
      m.setColorAt(n, colour);
    });
    m.count = picked.length;
    m.instanceMatrix.needsUpdate = true;
    if (m.instanceColor) m.instanceColor.needsUpdate = true;
    invalidate();
  }, [picked, grid, nx, ny, nz, d, coverage, show, invalidate]);
  return (
    <instancedMesh
      // A new mesh when the grid grows, since an instanced mesh's capacity is fixed.
      key={coverage.views.length}
      ref={mesh}
      args={[geometry, material, coverage.views.length]}
      frustumCulled={false}
    />
  );
}

const OUTLINE = new Color().setRGB(0.54, 0.56, 0.6);

function BoxOutline({ analysis }: { analysis: RigAnalysis }) {
  const { min, max } = analysis.grid;
  const centre = new Vector3((min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2);
  const size: [number, number, number] = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
  return (
    <lineSegments position={centre}>
      <edgesGeometry args={[new BoxGeometry(...size)]} />
      <lineBasicMaterial color={OUTLINE} />
    </lineSegments>
  );
}

export function RigView({
  analysis,
  show,
  selected,
  onSelect,
}: {
  analysis: RigAnalysis;
  show: VoxelShow;
  selected: string | null;
  onSelect: (id: string | null) => void;
}) {
  const far = analysis.grid.max[2];
  const height = far + 0.05;
  // Rx(π): the rig's +Z (its cameras' view) points down.
  const hang = useMemo(
    () => new Matrix4().makeTranslation(0, 0, height).multiply(new Matrix4().makeRotationX(Math.PI)),
    [height],
  );
  const reach = Math.max(0.3, height);
  return (
    <div className="h-full w-full" data-testid="rig-view">
      <SceneCanvas
        className="h-full w-full"
        label="Rig viewport"
        keyboard
        eye={[reach * 0.9, -reach * 1.1, reach * 0.9]}
        target={[0, 0, height / 2]}
        onPointerMissed={() => onSelect(null)}
      >
        <group matrix={hang} matrixAutoUpdate={false}>
          {analysis.eyes.map((e) => (
            <group key={e.id} matrix={e.rigFromProjection} matrixAutoUpdate={false}>
              <CameraFrustum
                borderRays={e.borderRays}
                depth={Math.min(0.15, analysis.layout.working_distance_m * 0.4)}
                active={selected === e.id}
                onSelect={() => onSelect(e.id)}
              />
            </group>
          ))}
          {analysis.fans.map((f) => (
            <group key={f.id} matrix={f.rigFromLaser} matrixAutoUpdate={false}>
              <LaserFan
                halfAngle={f.halfAngle}
                length={Math.min(f.length, far * 1.05)}
                active={selected === f.id}
                onSelect={() => onSelect(f.id)}
              />
            </group>
          ))}
          <BoxOutline analysis={analysis} />
          <Voxels analysis={analysis} show={show} />
        </group>
        <ambientLight intensity={0.8} />
        <directionalLight position={[1, -1, 2]} intensity={1.2} />
      </SceneCanvas>
    </div>
  );
}

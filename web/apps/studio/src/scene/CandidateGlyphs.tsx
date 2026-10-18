import type { Candidate } from "@etendue/wasm";
import { GIZMO_LAYER, matrixFromIso3, setLayer } from "@vitavision/three";
import { useSceneColors } from "@vitavision/three-react";
import { useEffect, useMemo } from "react";
import { Color, ConeGeometry, InstancedMesh, Matrix4, MeshBasicMaterial } from "three";

/** Cone size: 8 mm radius, 30 mm long. */
const RADIUS = 0.008;
const LENGTH = 0.03;

/**
 * Generated candidate poses as small cones at the sensor, pointing where it looks: green where a
 * pose is kept, amber where the robot reaches it but a camera does not see the target (or a
 * joint is at its limit, or near a singularity), red where the robot cannot reach it. On the
 * gizmo layer, so camera images do not show them. Place it in the world frame.
 */
export function CandidateGlyphs({ candidates }: { candidates: readonly Candidate[] }) {
  const colors = useSceneColors();
  const mesh = useMemo(() => {
    // The cone's axis is +Y, its tip at +LENGTH/2: turned to the sensor's +Z, tip first.
    const geometry = new ConeGeometry(RADIUS, LENGTH, 12).rotateX(Math.PI / 2).translate(0, 0, LENGTH / 2);
    const m = new InstancedMesh(geometry, new MeshBasicMaterial(), Math.max(1, candidates.length));
    m.count = candidates.length;
    const matrix = new Matrix4();
    const color = new Color();
    for (const [i, c] of candidates.entries()) {
      m.setMatrixAt(i, matrixFromIso3(c.world_se3_sensor, matrix));
      const tone =
        c.status.status === "ok" ? colors.normal : c.status.status === "unreachable" ? colors.defect : colors.warn;
      m.setColorAt(i, color.setStyle(tone));
    }
    m.instanceMatrix.needsUpdate = true;
    if (m.instanceColor) m.instanceColor.needsUpdate = true;
    m.computeBoundingSphere();
    setLayer(m, GIZMO_LAYER);
    return m;
  }, [candidates, colors]);
  useEffect(
    () => () => {
      mesh.geometry.dispose();
      mesh.material.dispose();
      mesh.dispose();
    },
    [mesh],
  );
  return <primitive object={mesh} />;
}

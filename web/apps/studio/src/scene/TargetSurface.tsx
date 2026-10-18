import type { EtendueScene, TargetSpec } from "@etendue/wasm";
import type { ThreeEvent } from "@react-three/fiber";
import { useEffect, useMemo } from "react";
import { BufferAttribute, BufferGeometry, Color, DoubleSide, MeshBasicMaterial, SRGBColorSpace } from "three";

/**
 * Paper and ink, the sRGB colours `@vitavision/three`'s `TargetBoard` prints in (242 and 26 of
 * 255): a physical object, the same in every theme, so not design tokens.
 */
const PAPER = new Color().setRGB(242 / 255, 242 / 255, 242 / 255, SRGBColorSpace);
const INK = new Color().setRGB(26 / 255, 26 / 255, 26 / 255, SRGBColorSpace);

/**
 * The surface of a target as the kernel draws it (`EtendueScene.targetMesh`): the pattern
 * calib-targets prints — chessboard, ChArUco markers, PuzzleBoard dots, the PuzzlePole's cylinder
 * — or plain paper. One mesh with a colour per triangle, unlit like `TargetBoard`, on the
 * physical layer, so camera images (`SensorImage`) show the true pattern.
 */
export function TargetSurface({
  session,
  target,
  onSelect,
}: {
  session: EtendueScene;
  /** The target (a new object when it is edited). */
  target: TargetSpec;
  onSelect?: (() => void) | undefined;
}) {
  const geometry = useMemo(() => {
    let mesh;
    try {
      mesh = session.targetMesh(target.id);
    } catch {
      return null;
    }
    // Unindexed, so each triangle carries its own colour.
    const n = mesh.indices.length;
    const positions = new Float32Array(n * 3);
    const normals = new Float32Array(n * 3);
    const colors = new Float32Array(n * 3);
    for (let i = 0; i < n; i++) {
      const v = mesh.indices[i]!;
      positions.set(mesh.positions.subarray(3 * v, 3 * v + 3), 3 * i);
      normals.set(mesh.normals.subarray(3 * v, 3 * v + 3), 3 * i);
      const c = mesh.dark[Math.floor(i / 3)] ? INK : PAPER;
      colors.set([c.r, c.g, c.b], 3 * i);
    }
    const g = new BufferGeometry();
    g.setAttribute("position", new BufferAttribute(positions, 3));
    g.setAttribute("normal", new BufferAttribute(normals, 3));
    g.setAttribute("color", new BufferAttribute(colors, 3));
    g.computeBoundingSphere();
    return g;
    // `target` changes when the target is edited; the mesh is the kernel's for the session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session, target]);
  const material = useMemo(() => new MeshBasicMaterial({ vertexColors: true, side: DoubleSide }), []);
  useEffect(() => () => geometry?.dispose(), [geometry]);
  useEffect(() => () => material.dispose(), [material]);
  if (!geometry) return null;
  return (
    <mesh
      geometry={geometry}
      material={material}
      {...(onSelect && {
        onClick: (e: ThreeEvent<MouseEvent>) => {
          e.stopPropagation();
          onSelect();
        },
      })}
    />
  );
}

/**
 * The head in 3-D, to scale: the sensor (tilted as set), the principal planes H and H′, the
 * entrance and exit pupils, the camera's field, the laser's fan, the plane of best focus, and the
 * window on the laser plane coloured by total blur. Every camera quantity is the kernel's (the
 * section's `solid` and cardinal points); the studio only places them. The camera looks along the
 * world's +Y, its image +X along the world's +X.
 */

import type { SceneColors } from "@vitavision/three";
import { LaserFan, SceneCanvas, useSceneColors } from "@vitavision/three-react";
import { useEffect, useMemo } from "react";
import {
  BufferGeometry,
  Color,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  Line,
  LineBasicMaterial,
  LineLoop,
  LineSegments,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  Quaternion,
  Vector3,
} from "three";

import type { HeadAnalysis } from "./analysis";
import { type HeadParams, laserPose } from "./design";

/** Camera body → world: body x → x, body y (down) → −z, body z (forward) → +y. */
const BODY = new Matrix4().makeBasis(new Vector3(1, 0, 0), new Vector3(0, 0, -1), new Vector3(0, 1, 0));

const VIRIDIS = [
  [68, 1, 84],
  [59, 82, 139],
  [33, 145, 140],
  [94, 201, 98],
  [253, 231, 37],
].map(([r, g, b]) => new Color(r! / 255, g! / 255, b! / 255));

function viridis(t: number): Color {
  const x = Math.min(1, Math.max(0, t)) * (VIRIDIS.length - 1);
  const i = Math.min(VIRIDIS.length - 2, Math.floor(x));
  return VIRIDIS[i]!.clone().lerp(VIRIDIS[i + 1]!, x - i);
}

const v3 = (p: readonly number[]) => new Vector3(p[0], p[1], p[2]);

function lines(points: Vector3[], colour: string, loop = false): Line {
  const geometry = new BufferGeometry().setFromPoints(points);
  const material = new LineBasicMaterial({ color: colour });
  return loop ? new LineLoop(geometry, material) : new Line(geometry, material);
}

function circle(z: number, r: number, colour: string, n = 64): Line {
  return lines(
    Array.from(
      { length: n },
      (_, i) => new Vector3(r * Math.cos((2 * Math.PI * i) / n), r * Math.sin((2 * Math.PI * i) / n), z),
    ),
    colour,
    true,
  );
}

function quad(corners: Vector3[], colour: string, opacity: number): Mesh {
  const geometry = new BufferGeometry().setFromPoints([
    corners[0]!,
    corners[1]!,
    corners[2]!,
    corners[0]!,
    corners[2]!,
    corners[3]!,
  ]);
  return new Mesh(
    geometry,
    new MeshBasicMaterial({ color: colour, transparent: true, opacity, side: DoubleSide, depthWrite: false }),
  );
}

/** The anatomy as one group in the camera's body frame. */
function build(analysis: HeadAnalysis, params: HeadParams, scene: SceneColors): Group {
  const colours = {
    sensor: scene.canvas,
    edge: scene.lineStrong,
    principal: scene.signal,
    pupil: scene.warn,
    axis: scene.muted,
    field: scene.muted,
    hidden: scene.line,
    focus: scene.normal,
  };
  const g = new Group();
  const { section, map, range } = analysis;
  const { solid, cardinal_points: cp } = section;
  const reach = range[1] * 1.08;

  // The sensor, and the principal planes and pupils on the axis.
  const sensor = solid.sensor.map(v3);
  g.add(quad(sensor, colours.sensor, 0.85), lines(sensor, colours.edge, true));
  const lens = Math.max(section.entrance_radius, section.exit_radius) * 1.6;
  g.add(
    circle(cp.front_principal, lens, colours.principal),
    circle(cp.rear_principal, lens, colours.principal),
    circle(cp.entrance_pupil, section.entrance_radius, colours.pupil),
    circle(cp.exit_pupil, section.exit_radius, colours.pupil),
    lines([new Vector3(0, 0, Math.min(0, cp.rear_principal) - 0.005), new Vector3(0, 0, reach)], colours.axis),
  );

  // The field: its edge rays out to the window's far end, and their ends.
  const ends = solid.field.map((r) => {
    const p = v3(r.point);
    const d = v3(r.direction);
    return p.clone().addScaledVector(d, (reach - p.z) / Math.max(1e-6, d.z));
  });
  const field = new BufferGeometry().setFromPoints(solid.field.flatMap((r, i) => [v3(r.point), ends[i]!]));
  g.add(new LineSegments(field, new LineBasicMaterial({ color: colours.field })), lines(ends, colours.field, true));

  // The window on the laser plane, coloured by total blur.
  const pose = laserPose(params);
  const origin = v3(pose.translation);
  const q = new Quaternion(...pose.rotation);
  const line = new Vector3(0, 1, 0).applyQuaternion(q);
  const central = new Vector3(0, 0, 1).applyQuaternion(q);
  const { across, along } = map;
  const [lo, hi] = (() => {
    let [a, b] = [Infinity, -Infinity];
    for (let k = 0; k < across * along; k++) {
      if (map.visible[k] && Number.isFinite(map.blur_px[k]!))
        [a, b] = [Math.min(a, map.blur_px[k]!), Math.max(b, map.blur_px[k]!)];
    }
    return Number.isFinite(a) ? [a, Math.max(b, a + 1e-9)] : [0, 1];
  })();
  const positions: number[] = [];
  const colourAt: number[] = [];
  const hidden = new Color(colours.hidden);
  for (let j = 0; j < along; j++) {
    const b = range[0] + (j / Math.max(1, along - 1)) * (range[1] - range[0]);
    for (let i = 0; i < across; i++) {
      const a = (i / Math.max(1, across - 1) - 0.5) * params.width_m;
      const p = origin.clone().addScaledVector(central, b).addScaledVector(line, a);
      positions.push(p.x, p.y, p.z);
      const k = j * across + i;
      const c = map.visible[k] ? viridis((map.blur_px[k]! - lo) / (hi - lo)) : hidden;
      colourAt.push(c.r, c.g, c.b);
    }
  }
  const index: number[] = [];
  for (let j = 0; j + 1 < along; j++) {
    for (let i = 0; i + 1 < across; i++) {
      const k = j * across + i;
      index.push(k, k + 1, k + across, k + 1, k + across + 1, k + across);
    }
  }
  const window = new BufferGeometry()
    .setAttribute("position", new Float32BufferAttribute(positions, 3))
    .setAttribute("color", new Float32BufferAttribute(colourAt, 3));
  window.setIndex(index);
  // In the fan's plane: drawn in front of it.
  g.add(
    new Mesh(
      window,
      new MeshBasicMaterial({
        vertexColors: true,
        side: DoubleSide,
        polygonOffset: true,
        polygonOffsetFactor: -2,
        polygonOffsetUnits: -2,
      }),
    ),
  );

  // The plane of best focus, around the window's centre.
  const n = v3(solid.focus.normal).normalize();
  const centre = origin.clone().addScaledVector(central, (range[0] + range[1]) / 2);
  const onPlane = centre.clone().addScaledVector(n, -centre.clone().sub(v3(solid.focus.point)).dot(n));
  const u = new Vector3(1, 0, 0).addScaledVector(n, -n.x).normalize();
  const w = new Vector3().crossVectors(n, u).normalize();
  const [hu, hw] = [params.width_m * 0.9, (range[1] - range[0]) * 0.75];
  g.add(
    quad(
      [
        onPlane.clone().addScaledVector(u, -hu).addScaledVector(w, -hw),
        onPlane.clone().addScaledVector(u, hu).addScaledVector(w, -hw),
        onPlane.clone().addScaledVector(u, hu).addScaledVector(w, hw),
        onPlane.clone().addScaledVector(u, -hu).addScaledVector(w, hw),
      ],
      colours.focus,
      0.18,
    ),
  );
  return g;
}

export function Anatomy3D({ analysis, params }: { analysis: HeadAnalysis; params: HeadParams }) {
  const scene = useSceneColors();
  const group = useMemo(() => build(analysis, params, scene), [analysis, params, scene]);
  useEffect(
    () => () =>
      group.traverse((o) => {
        if (o instanceof Mesh || o instanceof Line) {
          (o.geometry as BufferGeometry).dispose();
          (o.material as LineBasicMaterial | MeshBasicMaterial).dispose();
        }
      }),
    [group],
  );
  const laser = useMemo(() => {
    const pose = laserPose(params);
    return new Matrix4().compose(v3(pose.translation), new Quaternion(...pose.rotation), new Vector3(1, 1, 1));
  }, [params]);
  const far = analysis.range[1];
  return (
    <div className="h-full w-full" data-testid="anatomy-3d">
      <SceneCanvas
        className="h-full w-full"
        label="Head anatomy"
        keyboard
        // Above and beside the window, looking down onto the laser plane (near level here).
        eye={[far * 0.5, -far * 0.05, far * 0.8]}
        target={[0, far * 0.55, 0]}
      >
        <group matrix={BODY} matrixAutoUpdate={false}>
          <primitive object={group} />
          <group matrix={laser} matrixAutoUpdate={false}>
            <LaserFan halfAngle={params.laser.fan_half_angle} length={far * 1.08} />
          </group>
        </group>
        <ambientLight intensity={0.9} />
      </SceneCanvas>
    </div>
  );
}

/**
 * The calibration's estimate drawn beside the truth (P13), in the warning colour: each estimated
 * camera's frustum, its laser plane over the true fan, and the target's outline, each placed by
 * its (exaggerated) offset from the true frame. On the gizmo layer, so camera images do not
 * show them. Each part sits under its true frame's `AtFrame`.
 */

import type { Iso3Schema } from "@etendue/wasm";
import { CameraFrustum, GIZMO_LAYER, disposeObject, matrixFromIso3, setLayer } from "@vitavision/three";
import { AtFrame, useSceneColors } from "@vitavision/three-react";
import { useEffect, useMemo } from "react";
import {
  BufferGeometry,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  Line,
  LineBasicMaterial,
  Mesh,
  MeshBasicMaterial,
} from "three";

/** What to draw, already exaggerated. */
export interface EstimateDrawing {
  cameras: {
    id: string;
    /** The estimated camera in the true camera's frame. */
    offset: Iso3Schema;
    /** The estimated laser plane's patch over the true fan, true-camera coordinates: apex, then arc. */
    fan: Float32Array | null;
  }[];
  target: {
    id: string;
    /** The estimated target in the true target's frame. */
    offset: Iso3Schema;
    /** Its outline, target coordinates (flat `[x, y, z, …]`). */
    outline: Float64Array;
  } | null;
}

function useDisposed<T extends Group>(object: T): T {
  useEffect(() => () => disposeObject(object), [object]);
  return object;
}

function Frustum({ rays, offset, color }: { rays: Float64Array; offset: Iso3Schema; color: string }) {
  const object = useDisposed(
    useMemo(() => {
      const f = new CameraFrustum({ borderRays: rays, depth: 0.12, color, pickPadding: 1 });
      f.setActive(true);
      f.hitbox.visible = false;
      return f;
    }, [rays, color]),
  );
  const matrix = useMemo(() => matrixFromIso3(offset), [offset]);
  return <primitive object={object} matrix={matrix} matrixAutoUpdate={false} />;
}

/** A fan (apex, then arc) as a translucent sector with its outline. */
function Fan({ fan, color }: { fan: Float32Array; color: string }) {
  const object = useDisposed(
    useMemo(() => {
      const n = fan.length / 3;
      const triangles: number[] = [];
      for (let i = 1; i + 1 < n; i++) triangles.push(0, i, i + 1);
      const fill = new BufferGeometry().setAttribute("position", new Float32BufferAttribute(fan, 3));
      fill.setIndex(triangles);
      const outline = new BufferGeometry().setAttribute(
        "position",
        new Float32BufferAttribute([...fan, fan[0]!, fan[1]!, fan[2]!], 3),
      );
      const group = new Group();
      group.add(
        new Mesh(
          fill,
          new MeshBasicMaterial({ color, transparent: true, opacity: 0.12, side: DoubleSide, depthWrite: false }),
        ),
        new Line(outline, new LineBasicMaterial({ color })),
      );
      setLayer(group, GIZMO_LAYER);
      return group;
    }, [fan, color]),
  );
  return <primitive object={object} />;
}

function Outline({ outline, offset, color }: { outline: Float64Array; offset: Iso3Schema; color: string }) {
  const object = useDisposed(
    useMemo(() => {
      const geometry = new BufferGeometry().setAttribute("position", new Float32BufferAttribute(outline, 3));
      const group = new Group();
      group.add(new Line(geometry, new LineBasicMaterial({ color })));
      setLayer(group, GIZMO_LAYER);
      return group;
    }, [outline, color]),
  );
  const matrix = useMemo(() => matrixFromIso3(offset), [offset]);
  return <primitive object={object} matrix={matrix} matrixAutoUpdate={false} />;
}

/** `drawing` under the frame tree; `rays` are the cameras' border rays (the true models'). */
export function Estimates({ drawing, rays }: { drawing: EstimateDrawing; rays: Map<string, Float64Array> }) {
  const color = useSceneColors().warn;
  return (
    <group name="calibration-estimate">
      {drawing.cameras.map((c) => {
        const r = rays.get(c.id);
        return (
          <AtFrame key={c.id} name={c.id}>
            {r && <Frustum rays={r} offset={c.offset} color={color} />}
            {c.fan && <Fan fan={c.fan} color={color} />}
          </AtFrame>
        );
      })}
      {drawing.target && (
        <AtFrame name={drawing.target.id}>
          <Outline outline={drawing.target.outline} offset={drawing.target.offset} color={color} />
        </AtFrame>
      )}
    </group>
  );
}

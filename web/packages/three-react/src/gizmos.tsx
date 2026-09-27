import type { ThreeEvent } from "@react-three/fiber";
import {
  Axes,
  CameraFrustum as CameraFrustumObject,
  LaserFan as LaserFanObject,
  LightGizmo as LightGizmoObject,
  type LightShapeLike,
  TargetBoard as TargetBoardObject,
  disposeObject,
} from "@vitavision/three";
import { useEffect, useMemo } from "react";
import type { Object3D } from "three";

import { useSceneColors } from "./colors";

/** Colour an object is built with before its effect applies the theme's. */
const UNSET = "gray";

function useDisposed<T extends Object3D>(object: T): T {
  useEffect(() => () => disposeObject(object), [object]);
  return object;
}

function pick(onSelect: (() => void) | undefined) {
  if (!onSelect) return {};
  return {
    onClick: (e: ThreeEvent<MouseEvent>) => {
      e.stopPropagation();
      onSelect();
    },
    onPointerOver: (e: ThreeEvent<PointerEvent>) => {
      e.stopPropagation();
      document.body.style.cursor = "pointer";
    },
    onPointerOut: () => {
      document.body.style.cursor = "";
    },
  };
}

/** Props of {@link CameraFrustum}. */
export interface CameraFrustumProps {
  /** Border rays on the camera's `z = 1` plane (see `imageBorderPixels` in `@vitavision/three`). */
  borderRays: ArrayLike<number>;
  /** Far outline depth in metres. */
  depth: number;
  /** Emphasised (selected); drawn in the accent colour. */
  active?: boolean;
  /** Makes it pickable. */
  onSelect?: (() => void) | undefined;
}

/** A camera's field of view, in the camera frame (place it with `AtFrame`). */
export function CameraFrustum({ borderRays, depth, active = false, onSelect }: CameraFrustumProps) {
  const colors = useSceneColors();
  const object = useDisposed(useMemo(() => new CameraFrustumObject({ borderRays, depth, color: UNSET }), [borderRays, depth]));
  useEffect(() => {
    object.setActive(active);
    object.setColor(active ? colors.signal : colors.muted);
  }, [object, active, colors.signal, colors.muted]);
  return <primitive object={object} {...pick(onSelect)} />;
}

/** Props of {@link LaserFan}. */
export interface LaserFanProps {
  /** Half-angle in radians. */
  halfAngle: number;
  /** Reach in metres. */
  length: number;
  /** Makes it pickable. */
  onSelect?: (() => void) | undefined;
}

/** A line laser's light sheet, in the laser frame. Drawn in the `defect` (red) token. */
export function LaserFan({ halfAngle, length, onSelect }: LaserFanProps) {
  const colors = useSceneColors();
  const object = useDisposed(useMemo(() => new LaserFanObject({ halfAngle, length, color: UNSET }), [halfAngle, length]));
  useEffect(() => object.setColor(colors.defect), [object, colors.defect]);
  return <primitive object={object} {...pick(onSelect)} />;
}

/** Props of {@link TargetBoard}. */
export interface TargetBoardProps {
  /** Extent along local X, metres. */
  width: number;
  /** Extent along local Y, metres. */
  height: number;
  /** Checkerboard squares, columns along X. */
  checker?: { cols: number; rows: number } | undefined;
  /** Emphasised (selected). */
  active?: boolean;
  /** Makes it pickable. */
  onSelect?: (() => void) | undefined;
}

/** A planar target in the target frame (z = 0, facing +Z). */
export function TargetBoard({ width, height, checker, active = false, onSelect }: TargetBoardProps) {
  const colors = useSceneColors();
  const cols = checker?.cols;
  const rows = checker?.rows;
  const object = useDisposed(
    useMemo(
      () =>
        new TargetBoardObject({
          width,
          height,
          color: UNSET,
          edgeColor: UNSET,
          checker: cols !== undefined && rows !== undefined ? { cols, rows } : undefined,
        }),
      [width, height, cols, rows],
    ),
  );
  useEffect(() => object.setColors(colors.surface, active ? colors.signal : colors.fg), [object, active, colors]);
  return <primitive object={object} {...pick(onSelect)} />;
}

/** Props of {@link LightGizmo}. */
export interface LightGizmoProps {
  /** Emitter shape. */
  shape: LightShapeLike;
  /** Symbol size in metres. */
  size?: number;
  /** Makes it pickable. */
  onSelect?: (() => void) | undefined;
}

/** A light's wireframe symbol, in the light frame. Drawn in the `warn` token. */
export function LightGizmo({ shape, size = 0.05, onSelect }: LightGizmoProps) {
  const colors = useSceneColors();
  const key = JSON.stringify(shape);
  const object = useDisposed(useMemo(() => new LightGizmoObject(JSON.parse(key) as LightShapeLike, UNSET, size), [key, size]));
  useEffect(() => object.setColor(colors.warn), [object, colors.warn]);
  return <primitive object={object} {...pick(onSelect)} />;
}

/** Props of {@link FrameAxes}. */
export interface FrameAxesProps {
  /** Axis length in metres. */
  size?: number;
}

/** A frame's axes: X `defect`, Y `normal`, Z `signal`. */
export function FrameAxes({ size = 0.05 }: FrameAxesProps) {
  const colors = useSceneColors();
  const object = useDisposed(useMemo(() => new Axes(size, { x: UNSET, y: UNSET, z: UNSET }), [size]));
  useEffect(
    () => object.setColors({ x: colors.defect, y: colors.normal, z: colors.signal }),
    [object, colors.defect, colors.normal, colors.signal],
  );
  return <primitive object={object} />;
}

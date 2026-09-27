/*
 * A rigid transform, edited the way people think about one.
 *
 * Poses travel as `{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }` — the SE(3) wire
 * form — because a quaternion is the honest representation. Nobody edits one by hand,
 * though: a rotation is read and typed as roll, pitch and yaw in degrees. This component
 * shows that view and writes back the wire form.
 *
 * It owns no rotation mathematics. Which Euler convention "roll, pitch, yaw" means — the
 * axis order, intrinsic or extrinsic — is a decision of the 3D package that defines the
 * frames, so the conversion is passed in as a `RotationView`. This package stays pure UI.
 */

import { useState } from "react";

import { ReadoutStrip, byDensity, cn, useDensity } from "@vitavision/ui";

import { formatNumber } from "./numberText";
import { VectorInput } from "./VectorInput";

/** Three numbers: a translation `[tx, ty, tz]`, or Euler angles. */
export type Vec3 = readonly [number, number, number];

/** A unit quaternion in the wire order `[qx, qy, qz, qw]` (scalar last). */
export type Quaternion = readonly [number, number, number, number];

/**
 * An SE(3) pose in the wire form (`a_se3_b` as nalgebra's `Isometry3` serialises it):
 * translation in metres, rotation as a unit quaternion `[qx, qy, qz, qw]`.
 */
export interface PoseValue {
  /** The rotation, `[qx, qy, qz, qw]`. */
  rotation: Quaternion;
  /** The translation `[tx, ty, tz]`, in metres. */
  translation: Vec3;
}

/**
 * How a rotation is shown as three angles: the conversion both ways, and the angles' names.
 * Supplied by the package that owns the frame conventions; `PoseInput` only calls it.
 */
export interface RotationView {
  /** The quaternion as three angles, in radians, in the order of `labels`. */
  toEuler: (rotation: Quaternion) => Vec3;
  /** Three angles in radians, in the order of `labels`, as a unit quaternion `[qx, qy, qz, qw]`. */
  fromEuler: (angles: Vec3) => Quaternion;
  /** The angles' names, shown before each field. Defaults to roll, pitch, yaw. */
  labels?: readonly [string, string, string] | undefined;
}

/** Props of `PoseInput`. */
export interface PoseInputProps {
  /** The pose, in the wire form. */
  value: PoseValue;
  /** Called with a new pose (new arrays) whenever a component changes. */
  onValueChange?: ((value: PoseValue) => void) | undefined;
  /** The quaternion ↔ angles conversion and the angles' names. Required: see `RotationView`. */
  rotationView: RotationView;
  /** Show and edit the translation in metres (the default) or millimetres. The value is always metres. */
  translationUnit?: "m" | "mm" | undefined;
  /** Decimals of the translation. Defaults to 4 for m, 1 for mm. */
  translationPrecision?: number | undefined;
  /** Decimals of the angles, in degrees. Defaults to 2. */
  rotationPrecision?: number | undefined;
  /** Show a compact mono readout instead of fields. */
  readOnly?: boolean | undefined;
  /** Blocks every field. */
  disabled?: boolean | undefined;
  /** Names the pose (`role="group"`). Defaults to "Pose". */
  "aria-label"?: string | undefined;
  /** Merged with the component's own classes through `cn`. */
  className?: string | undefined;
}

const DEGREES = 180 / Math.PI;
const DEFAULT_ANGLE_LABELS = ["roll", "pitch", "yaw"] as const;

const sameQuaternion = (a: Quaternion, b: Quaternion) => a.every((component, index) => component === b[index]);

/**
 * An SE(3) pose shown and edited as a translation (m or mm) and three angles in degrees,
 * reading and writing the wire form `{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }`.
 *
 * The angles come from `rotationView.toEuler` and go back through `rotationView.fromEuler`.
 * The angles as typed are kept while the pose they produced is the one shown, so a yaw of
 * 190° is not redisplayed as −170° and a gimbal-locked pitch does not scramble the other two
 * angles mid-edit.
 *
 * A `role="group"` named by `aria-label`, holding two `VectorInput` groups ("Translation",
 * "Rotation"). With `readOnly` it renders a `ReadoutStrip` instead (`t 0.1000 0.2000 0.3000 m ·
 * rpy 0.00 90.00 0.00 °`). The root carries `data-readonly`.
 */
export function PoseInput({
  value,
  onValueChange,
  rotationView,
  translationUnit = "m",
  translationPrecision,
  rotationPrecision = 2,
  readOnly = false,
  disabled = false,
  "aria-label": ariaLabel = "Pose",
  className,
}: PoseInputProps) {
  const density = useDensity();
  // The angles last typed, and the quaternion they became.
  const [typed, setTyped] = useState<{ rotation: Quaternion; angles: Vec3 } | null>(null);

  const scale = translationUnit === "mm" ? 1000 : 1;
  const tPrecision = translationPrecision ?? (translationUnit === "mm" ? 1 : 4);
  const translation = value.translation.map((component) => component * scale);
  const radians =
    typed !== null && sameQuaternion(typed.rotation, value.rotation) ? typed.angles : rotationView.toEuler(value.rotation);
  const degrees = radians.map((angle) => angle * DEGREES);
  const angleLabels = rotationView.labels ?? DEFAULT_ANGLE_LABELS;

  if (readOnly) {
    const join = (values: readonly number[], precision: number) =>
      values.map((component) => formatNumber(component, precision)).join(" ");
    return (
      <div role="group" aria-label={ariaLabel} data-readonly="" className={cn("min-w-0", className)}>
        <ReadoutStrip
          items={[
            { label: "t", value: `${join(translation, tPrecision)} ${translationUnit}` },
            { label: "rpy", value: `${join(degrees, rotationPrecision)} °` },
          ]}
        />
      </div>
    );
  }

  const onTranslation = (next: number[]) => {
    const [x = 0, y = 0, z = 0] = next.map((component) => component / scale);
    onValueChange?.({ rotation: value.rotation, translation: [x, y, z] });
  };

  const onRotation = (next: number[]) => {
    const [a = 0, b = 0, c = 0] = next.map((angle) => angle / DEGREES);
    const angles: Vec3 = [a, b, c];
    const rotation = rotationView.fromEuler(angles);
    setTyped({ rotation, angles });
    onValueChange?.({ rotation, translation: value.translation });
  };

  const rowLabel = cn("text-fg-muted", byDensity(density, "text-xs", "text-[11px]"));

  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className={cn("grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2", byDensity(density, "gap-y-2", "gap-y-1"), className)}
    >
      <span aria-hidden className={rowLabel}>
        Translation
      </span>
      <VectorInput
        aria-label="Translation"
        value={translation}
        onValueChange={onTranslation}
        unit={translationUnit}
        precision={tPrecision}
        step={translationUnit === "mm" ? 1 : 0.001}
        disabled={disabled}
      />
      <span aria-hidden className={rowLabel}>
        Rotation
      </span>
      <VectorInput
        aria-label="Rotation"
        value={degrees}
        onValueChange={onRotation}
        labels={angleLabels}
        unit="°"
        precision={rotationPrecision}
        step={1}
        disabled={disabled}
      />
    </div>
  );
}

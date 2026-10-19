import type { Iso3Schema } from "@etendue/wasm";
import { quaternionFromRpy, rpyFromQuaternion } from "@vitavision/three";
import { NumberInput, Slider } from "@vitavision/ui";
import type { KeyboardEvent } from "react";

import { RowLabel, Rows } from "./Fold";

/**
 * An edit that is shown while it is made (a drag, typing) and recorded once: `preview` on every
 * move, `commit` when the drag ends or the field is left, `cancel` on Escape.
 */
export interface Live<T> {
  preview: (value: T) => void;
  commit: () => void;
  cancel: () => void;
}

const DEG = 180 / Math.PI;

/** The keys that end typing into a field: Enter keeps the value, Escape drops it. */
function endKeys(live: Live<never> | Live<number>) {
  return (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") live.commit();
    if (e.key === "Escape") live.cancel();
  };
}

/**
 * A quantity as a slider and its exact value beside it. Shown in display units
 * (`value · scale`): metres as millimetres, radians as degrees.
 */
export function SliderRow({
  label,
  value,
  range,
  limits,
  step,
  unit,
  scale = 1,
  precision = 1,
  live,
  testId,
}: {
  label: string;
  /** In stored units. */
  value: number;
  /** The slider's span, in display units. */
  range: readonly [number, number];
  /** What the field accepts, in display units (default: anything). */
  limits?: readonly [number, number] | undefined;
  /** The slider's step, in display units (default: a thousandth of the span). */
  step?: number | undefined;
  unit?: string | undefined;
  scale?: number | undefined;
  precision?: number | undefined;
  /** `null`: read only. */
  live: Live<number> | null;
  testId?: string | undefined;
}) {
  const shown = value * scale;
  return (
    <>
      <RowLabel>{label}</RowLabel>
      <div className="min-w-0" data-testid={testId}>
        <Slider
          aria-label={label}
          className="min-w-0"
          value={Math.min(range[1], Math.max(range[0], shown))}
          min={range[0]}
          max={range[1]}
          step={step ?? (range[1] - range[0]) / 1000}
          disabled={live === null}
          onValueChange={(v) => live?.preview(v / scale)}
          onValueCommit={() => live?.commit()}
          readout={
            <div className="w-[6.25rem] shrink-0">
              <NumberInput
                aria-label={`${label} value`}
                className="h-6 text-xs tabular-nums"
                value={shown}
                precision={precision}
                unit={unit}
                min={limits?.[0]}
                max={limits?.[1]}
                step="any"
                readOnly={live === null}
                onValueChange={(v) => live?.preview(v / scale)}
                onBlur={() => live?.commit()}
                onKeyDown={live ? endKeys(live) : undefined}
              />
            </div>
          }
        />
      </div>
    </>
  );
}

/** A quantity as an exact value only (no natural slider span). */
export function NumberRow({
  label,
  value,
  limits,
  unit,
  scale = 1,
  precision,
  integer = false,
  live,
}: {
  label: string;
  value: number;
  limits?: readonly [number, number] | undefined;
  unit?: string | undefined;
  scale?: number | undefined;
  precision?: number | undefined;
  integer?: boolean | undefined;
  live: Live<number> | null;
}) {
  return (
    <>
      <RowLabel title={label}>{label}</RowLabel>
      <div className="w-[8rem]">
        <NumberInput
          aria-label={label}
          className="h-6 text-xs tabular-nums"
          value={value * scale}
          precision={integer ? 0 : precision}
          unit={unit}
          min={limits?.[0]}
          max={limits?.[1]}
          step={integer ? 1 : "any"}
          readOnly={live === null}
          onValueChange={(v) => live?.preview((integer ? Math.round(v) : v) / scale)}
          onBlur={() => live?.commit()}
          onKeyDown={live ? endKeys(live) : undefined}
        />
      </div>
    </>
  );
}

const AXES = ["x", "y", "z"] as const;
const ANGLES = [
  { label: "roll", range: [-180, 180] },
  { label: "pitch", range: [-90, 90] },
  { label: "yaw", range: [-180, 180] },
] as const;

/**
 * A pose as six sliders: translation in millimetres, within `span` of `centre` (the committed
 * pose, so the track stays put while it is dragged), and roll, pitch, yaw in degrees (URDF
 * order, the one `@vitavision/three` defines).
 */
export function PoseRows({
  value,
  centre,
  span,
  live,
  testId,
}: {
  value: Iso3Schema;
  centre: Iso3Schema;
  /** Half the translation sliders' span, in millimetres. */
  span: number;
  live: Live<Iso3Schema> | null;
  testId?: string | undefined;
}) {
  const t = value.translation;
  const rpy = rpyFromQuaternion(value.rotation);
  const axis = (i: number): Live<number> | null =>
    live && {
      ...live,
      preview: (v) =>
        live.preview({
          rotation: [...value.rotation],
          translation: t.map((x, j) => (j === i ? v : x)) as Iso3Schema["translation"],
        }),
    };
  const angle = (i: number): Live<number> | null =>
    live && {
      ...live,
      preview: (v) =>
        live.preview({
          rotation: quaternionFromRpy(rpy.map((x, j) => (j === i ? v : x))),
          translation: [...t],
        }),
    };
  return (
    <div data-testid={testId}>
      <Rows>
        {AXES.map((a, i) => {
          const c = centre.translation[i]! * 1000;
          return (
            <SliderRow
              key={a}
              label={a}
              value={t[i]!}
              scale={1000}
              unit="mm"
              precision={span >= 1000 ? 0 : span >= 100 ? 1 : 2}
              range={[c - span, c + span]}
              live={axis(i)}
              testId={testId && `${testId}-${a}`}
            />
          );
        })}
        {ANGLES.map((a, i) => (
          <SliderRow
            key={a.label}
            label={a.label}
            value={rpy[i]!}
            scale={DEG}
            unit="°"
            precision={2}
            range={a.range}
            step={0.05}
            live={angle(i)}
            testId={testId && `${testId}-${a.label}`}
          />
        ))}
      </Rows>
    </div>
  );
}

/** A pose as one line: millimetres and degrees. */
export function poseSummary(pose: Iso3Schema): string {
  const t = pose.translation.map((x) => (x * 1000).toFixed(1)).join(", ");
  const r = rpyFromQuaternion(pose.rotation)
    .map((x) => (x * DEG).toFixed(1))
    .join(", ");
  return `${t} mm · ${r}°`;
}

/** A pose, read only: translation and roll, pitch, yaw in two aligned rows. */
export function PoseReadout({ pose, testId }: { pose: Iso3Schema; testId?: string | undefined }) {
  const rpy = rpyFromQuaternion(pose.rotation);
  const cell = "text-right font-mono text-xs tabular-nums";
  return (
    <div className="grid grid-cols-[3.25rem_repeat(3,minmax(0,1fr))] gap-x-2 gap-y-0.5" data-testid={testId}>
      <RowLabel>mm</RowLabel>
      {pose.translation.map((x, i) => (
        <span key={AXES[i]} className={cell} title={AXES[i]}>
          {(x * 1000).toFixed(2)}
        </span>
      ))}
      <RowLabel>°</RowLabel>
      {rpy.map((x, i) => (
        <span key={ANGLES[i]!.label} className={cell} title={ANGLES[i]!.label}>
          {(x * DEG).toFixed(2)}
        </span>
      ))}
    </div>
  );
}

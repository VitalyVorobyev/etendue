/**
 * Forms for the simple library parts (sensor, lens, laser, mount), in the units a datasheet uses,
 * and for what every item carries: its name, description and source.
 */

import type { ItemSource, LaserPart, LensPart, LibraryItem, MountPart, SensorPart } from "@etendue/wasm";
import { quaternionFromRpy, rpyFromQuaternion } from "@vitavision/three";
import { Field, Input, NumberInput, PoseInput, Select, Textarea, VectorInput } from "@vitavision/ui";

const DEG = 180 / Math.PI;

/** `part` with `key` set to `value`, or without `key` when `value` is `null`. */
function withField<T extends object, K extends keyof T>(part: T, key: K, value: T[K] | null): T {
  const next = { ...part };
  if (value === null) delete next[key];
  else next[key] = value;
  return next;
}

/** A number field in display units: `scale` turns the stored SI value into them. */
function Num({
  label,
  value,
  onChange,
  unit,
  scale = 1,
  precision,
  min,
  max,
  optional = false,
  disabled,
}: {
  label: string;
  value: number | null | undefined;
  onChange: (v: number | null) => void;
  unit?: string;
  scale?: number;
  precision?: number;
  min?: number;
  max?: number;
  /** May be left empty (unknown). */
  optional?: boolean;
  disabled: boolean;
}) {
  return (
    <Field label={label} annotation={optional ? "optional" : undefined}>
      <NumberInput
        value={value == null ? null : value * scale}
        onValueChange={(v) => onChange(v / scale)}
        onClear={optional ? () => onChange(null) : undefined}
        unit={unit}
        precision={precision}
        min={min}
        max={max}
        disabled={disabled}
      />
    </Field>
  );
}

const GRID = "grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-3";

export function SensorForm({
  part,
  onChange,
  disabled,
}: {
  part: SensorPart;
  onChange: (p: SensorPart) => void;
  disabled: boolean;
}) {
  return (
    <div className={GRID}>
      <Num
        label="Pixel pitch"
        unit="µm"
        scale={1e6}
        precision={3}
        min={0}
        value={part.pixel_pitch_m}
        onChange={(v) => v !== null && onChange({ ...part, pixel_pitch_m: v })}
        disabled={disabled}
      />
      <Field label="Resolution" as="group">
        <VectorInput
          aria-label="Resolution"
          labels={["w", "h"]}
          unit="px"
          precision={0}
          min={1}
          step={1}
          value={part.resolution}
          onValueChange={(v) => onChange({ ...part, resolution: [Math.round(v[0]!), Math.round(v[1]!)] })}
          disabled={disabled}
        />
      </Field>
      <Num
        label="Full well"
        unit="e⁻"
        precision={0}
        min={0}
        optional
        value={part.well_depth_e}
        onChange={(v) => onChange(withField(part, "well_depth_e", v))}
        disabled={disabled}
      />
      <Num
        label="Read noise"
        unit="e⁻"
        precision={2}
        min={0}
        optional
        value={part.read_noise_e}
        onChange={(v) => onChange(withField(part, "read_noise_e", v))}
        disabled={disabled}
      />
      <Num
        label="Peak QE"
        precision={2}
        min={0}
        max={1}
        optional
        value={part.peak_qe}
        onChange={(v) => onChange(withField(part, "peak_qe", v))}
        disabled={disabled}
      />
    </div>
  );
}

const MOUNTS: { value: LensPart["mount"]; label: string }[] = [
  { value: "c_mount", label: "C-mount" },
  { value: "cs_mount", label: "CS-mount" },
  { value: "f_mount", label: "F-mount" },
  { value: "m12", label: "M12" },
  { value: "other", label: "Other" },
];

export function LensForm({
  part,
  onChange,
  disabled,
}: {
  part: LensPart;
  onChange: (p: LensPart) => void;
  disabled: boolean;
}) {
  return (
    <div className={GRID}>
      <Num
        label="Focal length"
        unit="mm"
        scale={1e3}
        precision={2}
        min={0}
        value={part.focal_length_m}
        onChange={(v) => v !== null && onChange({ ...part, focal_length_m: v })}
        disabled={disabled}
      />
      <Num
        label="Widest f-number"
        precision={2}
        min={0}
        value={part.min_f_number}
        onChange={(v) => v !== null && onChange({ ...part, min_f_number: v })}
        disabled={disabled}
      />
      <Field label="Mount">
        <Select
          value={part.mount}
          options={MOUNTS}
          disabled={disabled}
          onValueChange={(v) => {
            const mount = MOUNTS.find((m) => m.value === v)?.value;
            if (mount) onChange({ ...part, mount });
          }}
        />
      </Field>
      <Num
        label="Diagonal field of view"
        unit="°"
        precision={1}
        min={0}
        optional
        value={part.fov_diagonal_deg}
        onChange={(v) => onChange(withField(part, "fov_diagonal_deg", v))}
        disabled={disabled}
      />
      <Num
        label="MTF50"
        unit="lp/mm"
        precision={0}
        min={0}
        optional
        value={part.mtf50_lp_mm}
        onChange={(v) => onChange(withField(part, "mtf50_lp_mm", v))}
        disabled={disabled}
      />
      <Num
        label="Principal-plane gap"
        unit="mm"
        scale={1e3}
        precision={2}
        value={part.principal_gap_m ?? 0}
        onChange={(v) => v !== null && onChange({ ...part, principal_gap_m: v })}
        disabled={disabled}
      />
      <Num
        label="Entrance pupil from H"
        unit="mm"
        scale={1e3}
        precision={2}
        value={part.entrance_pupil_m ?? 0}
        onChange={(v) => v !== null && onChange({ ...part, entrance_pupil_m: v })}
        disabled={disabled}
      />
    </div>
  );
}

export function LaserForm({
  part,
  onChange,
  disabled,
}: {
  part: LaserPart;
  onChange: (p: LaserPart) => void;
  disabled: boolean;
}) {
  return (
    <div className={GRID}>
      <Num
        label="Wavelength"
        unit="nm"
        precision={0}
        min={0}
        value={part.wavelength_nm}
        onChange={(v) => v !== null && onChange({ ...part, wavelength_nm: v })}
        disabled={disabled}
      />
      <Num
        label="Fan half-angle"
        unit="°"
        scale={DEG}
        precision={2}
        min={0}
        max={90}
        value={part.fan_half_angle}
        onChange={(v) => v !== null && onChange({ ...part, fan_half_angle: v })}
        disabled={disabled}
      />
      <Num
        label="Power"
        unit="mW"
        precision={1}
        min={0}
        optional
        value={part.power_mw}
        onChange={(v) => onChange(withField(part, "power_mw", v))}
        disabled={disabled}
      />
      <Num
        label="Beam waist (1/e² radius)"
        unit="µm"
        scale={1e6}
        precision={1}
        min={0}
        optional
        value={part.beam_waist_m}
        onChange={(v) => onChange(withField(part, "beam_waist_m", v))}
        disabled={disabled}
      />
      <Num
        label="Waist distance"
        unit="m"
        precision={3}
        min={0}
        optional
        value={part.waist_distance_m}
        onChange={(v) => onChange(withField(part, "waist_distance_m", v))}
        disabled={disabled}
      />
    </div>
  );
}

const RPY = { toEuler: rpyFromQuaternion, fromEuler: quaternionFromRpy, labels: ["roll", "pitch", "yaw"] } as const;

export function MountForm({
  part,
  onChange,
  disabled,
}: {
  part: MountPart;
  onChange: (p: MountPart) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-3">
      <Field label="Flange to device" as="group" description="Where the device sits in the tool flange's frame.">
        <PoseInput
          aria-label="Flange to device"
          value={part.flange_se3_device}
          rotationView={RPY}
          translationUnit="mm"
          onValueChange={(v) =>
            onChange({ ...part, flange_se3_device: { rotation: [...v.rotation], translation: [...v.translation] } })
          }
          disabled={disabled}
        />
      </Field>
      <div className={GRID}>
        <Num
          label="Mass"
          unit="kg"
          precision={3}
          min={0}
          optional
          value={part.mass_kg}
          onChange={(v) => onChange(withField(part, "mass_kg", v))}
          disabled={disabled}
        />
      </div>
    </div>
  );
}

/** `text` as an optional string field: empty is absent. */
const optional = (text: string): string | null => (text.trim() === "" ? null : text);

/** Name and description. */
export function GeneralForm({
  item,
  onChange,
  disabled,
}: {
  item: LibraryItem;
  onChange: (i: LibraryItem) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-3">
      <Field label="Name" error={item.name.trim() === "" ? "A name is needed." : undefined}>
        <Input value={item.name} disabled={disabled} onChange={(e) => onChange({ ...item, name: e.target.value })} />
      </Field>
      <Field label="Description">
        <Textarea
          value={item.description ?? ""}
          minRows={2}
          maxRows={8}
          disabled={disabled}
          onValueChange={(v) => onChange(withField(item, "description", optional(v)))}
        />
      </Field>
    </div>
  );
}

/** Where the data comes from: vendor, part number, datasheet, notes. */
export function SourceForm({
  item,
  onChange,
  disabled,
}: {
  item: LibraryItem;
  onChange: (i: LibraryItem) => void;
  disabled: boolean;
}) {
  const source: ItemSource = item.source ?? {};
  const set = (key: keyof ItemSource, text: string) => {
    const next = withField(source, key, optional(text));
    onChange(withField(item, "source", Object.keys(next).length === 0 ? null : next));
  };
  return (
    <div className="space-y-3">
      <div className={GRID}>
        <Field label="Vendor">
          <Input value={source.vendor ?? ""} disabled={disabled} onChange={(e) => set("vendor", e.target.value)} />
        </Field>
        <Field label="Part number">
          <Input
            value={source.part_number ?? ""}
            disabled={disabled}
            onChange={(e) => set("part_number", e.target.value)}
          />
        </Field>
      </div>
      <Field label="Datasheet" annotation="URL">
        <Input
          type="url"
          value={source.datasheet_url ?? ""}
          disabled={disabled}
          onChange={(e) => set("datasheet_url", e.target.value)}
        />
      </Field>
      <Field label="Notes" description="How a value was measured, what was assumed.">
        <Textarea
          value={source.notes ?? ""}
          minRows={2}
          maxRows={8}
          disabled={disabled}
          onValueChange={(v) => set("notes", v)}
        />
      </Field>
    </div>
  );
}

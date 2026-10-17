import type { CameraSpec, LaserLayout, RigLayout, SceneSpec } from "@etendue/wasm";
import { Button, NumberInput, SegmentedControl, Section, Select, Switch } from "@vitavision/ui";
import { LayoutGrid } from "lucide-react";
import type { ReactNode } from "react";

import { expandRig } from "../kernel/etendue";
import { applyLayout } from "../scene/edit";
import type { OnEdit } from "./Inspector";

const DEG = 180 / Math.PI;

const PRESETS = [
  { value: "1", label: "Mono" },
  { value: "2", label: "Stereo" },
  { value: "3", label: "3" },
  { value: "6", label: "6" },
];

const DEFAULT_LASERS: LaserLayout = {
  radius_m: 0.02,
  azimuth_rad: 0,
  fan_half_angle: (20 * Math.PI) / 180,
  fan_length: 1,
  wavelength_nm: 660,
  beam_waist_m: 0.0005,
};

/** A first layout for `rig`: as many cameras as it has (at least one), modelled on the first. */
function initialLayout(scene: SceneSpec, rig: string, template: CameraSpec | undefined): RigLayout {
  const count = Math.max(1, (scene.cameras ?? []).filter((c) => c.parent === rig).length);
  const camera = template ?? {
    params: {
      projection: { type: "pinhole" },
      distortion: { type: "none" },
      sensor: { type: "identity" },
      intrinsics: { type: "fx_fy_cx_cy_skew", fx: 2400, fy: 2400, cx: 640, cy: 512, skew: 0 },
    },
    resolution: [1280, 1024],
  };
  return {
    count,
    radius_m: count === 1 ? 0 : 0.06,
    phase_rad: 0,
    working_distance_m: 0.4,
    image_x: "tangential",
    camera: { params: camera.params, resolution: camera.resolution },
  };
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="self-center text-xs text-fg-muted">{label}</span>
      <div className="min-w-0">{children}</div>
    </>
  );
}

function Num({
  label,
  value,
  onChange,
  unit,
  scale = 1,
  precision = 1,
  min,
  max,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  unit: string;
  scale?: number;
  precision?: number;
  min?: number;
  max?: number;
}) {
  return (
    <Row label={label}>
      <NumberInput
        aria-label={label}
        value={value * scale}
        unit={unit}
        precision={precision}
        min={min}
        max={max}
        step="any"
        onValueChange={(v) => onChange(v / scale)}
      />
    </Row>
  );
}

/**
 * The rig builder: mono, stereo, or a ring of cameras around the rig's +Z axis, converging on a
 * working point, each with its own line laser if asked. Every change lays the rig out again (one
 * undoable edit per change): its cameras and lasers are replaced by the layout's.
 */
export function RigBuilder({ scene, rig, onEdit }: { scene: SceneSpec; rig: string; onEdit: OnEdit }) {
  const spec = scene.rigs?.find((r) => r.id === rig);
  if (!spec) return null;
  const onRig = (scene.cameras ?? []).filter((c) => c.parent === rig);
  const apply = (layout: RigLayout, label: string) =>
    onEdit(label, (d) => ({ ...d, scene: applyLayout(d.scene, rig, layout, expandRig(layout, rig)) }));

  const layout = spec.layout;
  if (!layout) {
    return (
      <Section title="Layout">
        <p className="mb-2 text-xs text-fg-muted">
          Lay the rig out as a mono, stereo or ring sensor. Its {onRig.length} camera(s) and its lasers are replaced by
          the layout's.
        </p>
        <Button
          size="sm"
          icon={<LayoutGrid className="size-4" />}
          onClick={() => apply(initialLayout(scene, rig, onRig[0]), `lay out ${rig}`)}
        >
          Lay out cameras
        </Button>
      </Section>
    );
  }
  // One undo step per field: a burst of edits of one field (typing) folds into one.
  const set = (patch: Partial<RigLayout>) =>
    apply({ ...layout, ...patch }, `set ${rig} ${Object.keys(patch).join(", ").replaceAll("_", " ")}`);
  const converging = layout.tilt_rad === undefined || layout.tilt_rad === null;
  const lasers = layout.lasers ?? null;
  const setLasers = (patch: Partial<LaserLayout>) => lasers && set({ lasers: { ...lasers, ...patch } });
  const preset = PRESETS.some((p) => p.value === String(layout.count)) ? String(layout.count) : "";
  return (
    <Section title="Layout" hint={`${layout.count} camera${layout.count === 1 ? "" : "s"}`}>
      <div className="space-y-2">
        <SegmentedControl
          value={preset}
          options={PRESETS}
          aria-label="Cameras"
          onValueChange={(v) => {
            if (v === "") return;
            const count = Number(v);
            set({ count, radius_m: count === 1 ? 0 : layout.radius_m > 0 ? layout.radius_m : 0.06 });
          }}
        />
        <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">
          <Row label="cameras">
            <NumberInput
              aria-label="Number of cameras"
              value={layout.count}
              precision={0}
              min={1}
              max={12}
              step={1}
              onValueChange={(v) => {
                const count = Math.round(v);
                set({ count, radius_m: count > 1 && layout.radius_m === 0 ? 0.06 : layout.radius_m });
              }}
            />
          </Row>
          <Num
            label="radius"
            value={layout.radius_m}
            scale={1000}
            unit="mm"
            min={0}
            onChange={(radius_m) => set({ radius_m })}
          />
          <Num
            label="working distance"
            value={layout.working_distance_m}
            scale={1000}
            unit="mm"
            precision={0}
            min={1}
            onChange={(working_distance_m) => set({ working_distance_m })}
          />
          <Row label="tilt">
            <div className="flex items-center gap-2">
              <Switch
                checked={converging}
                onCheckedChange={(on) =>
                  set({ tilt_rad: on ? null : Math.atan2(layout.radius_m, layout.working_distance_m) })
                }
                label="converge"
              />
              {!converging && (
                <NumberInput
                  aria-label="Tilt"
                  value={(layout.tilt_rad ?? 0) * DEG}
                  unit="°"
                  precision={1}
                  min={0}
                  max={89.9}
                  step="any"
                  onValueChange={(v) => set({ tilt_rad: v / DEG })}
                />
              )}
              {converging && (
                <span className="font-mono text-xs text-fg-muted">
                  {(Math.atan2(layout.radius_m, layout.working_distance_m) * DEG).toFixed(1)}°
                </span>
              )}
            </div>
          </Row>
          <Num
            label="phase"
            value={layout.phase_rad ?? 0}
            scale={DEG}
            unit="°"
            onChange={(phase_rad) => set({ phase_rad })}
          />
          <Row label="image x">
            <Select
              aria-label="Image x direction"
              value={layout.image_x ?? "tangential"}
              options={[
                { value: "tangential", label: "Along the ring" },
                { value: "radial", label: "Away from the axis" },
              ]}
              onValueChange={(v) => (v === "tangential" || v === "radial") && set({ image_x: v })}
            />
          </Row>
          {(scene.cameras ?? []).length > 0 && (
            <Row label="camera">
              <Select
                aria-label="Camera model"
                value=""
                placeholder="Copy a camera's model…"
                options={(scene.cameras ?? []).map((c) => ({
                  value: c.id,
                  label: c.id,
                  note: `${c.resolution[0]}×${c.resolution[1]}`,
                }))}
                onValueChange={(id) => {
                  const c = scene.cameras?.find((x) => x.id === id);
                  if (c) set({ camera: { params: c.params, resolution: c.resolution } });
                }}
              />
            </Row>
          )}
        </div>
        <Switch
          checked={lasers !== null}
          onCheckedChange={(on) => set({ lasers: on ? DEFAULT_LASERS : null })}
          label="One laser per camera"
          description="Each laser aims at the working point, its line across its camera's image."
        />
        {lasers && (
          <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">
            <Num
              label="laser radius"
              value={lasers.radius_m}
              scale={1000}
              unit="mm"
              min={0}
              onChange={(radius_m) => setLasers({ radius_m })}
            />
            <Num
              label="azimuth"
              value={lasers.azimuth_rad ?? 0}
              scale={DEG}
              unit="°"
              onChange={(azimuth_rad) => setLasers({ azimuth_rad })}
            />
            <Num
              label="fan"
              value={2 * lasers.fan_half_angle}
              scale={DEG}
              unit="°"
              min={0.1}
              max={179}
              onChange={(v) => setLasers({ fan_half_angle: v / 2 })}
            />
            <Num
              label="λ"
              value={lasers.wavelength_nm}
              unit="nm"
              precision={0}
              min={200}
              onChange={(wavelength_nm) => setLasers({ wavelength_nm })}
            />
          </div>
        )}
      </div>
    </Section>
  );
}

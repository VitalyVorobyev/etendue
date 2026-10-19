/**
 * Two slices through the rig's coverage, to scale: the side (x–z through the rig axis) and the
 * top (x–y at the working distance). Each voxel is coloured by how many cameras see it; voxels a
 * pair measures are marked in the laser's colour. Over them, the cameras (dimmed when out of
 * the slice's plane), their axes, and on the side each camera's sharp range along its axis (its
 * depth-of-field band for the blur limit). Handles drag the working distance, ρ, φ, the phase and
 * the box.
 */

import type { Sharpness } from "@etendue/wasm";
import { type PointerEvent as ReactPointerEvent, type ReactNode, useMemo, useRef } from "react";
import { Quaternion, Vector3 } from "three";

import type { RigAnalysis } from "./analysis";
import { type RigParams, tiltOf, withWorkingDistance } from "./design";

/** One colour per count, 1 to 8 views, cool to warm (viridis into plasma), as RGB. */
const VIEW_RGB: [number, number, number][] = [
  [59, 82, 139],
  [33, 145, 140],
  [94, 201, 98],
  [181, 222, 43],
  [253, 231, 37],
  [252, 165, 10],
  [241, 96, 93],
  [225, 51, 66],
];
const css = ([r, g, b]: [number, number, number]) => `rgb(${r},${g},${b})`;
/** One CSS colour per count. */
export const VIEW_COLOURS = VIEW_RGB.map(css);
/** Voxels a (laser, camera) pair measures. */
export const MEASURED = css([228, 87, 46]);

/** A point of a slice's drawing (m): side `[x, z]`, top `[x, −y]` (+y up). */
type Pt = [number, number];
type Handle = "working" | "radius" | "tilt" | "near" | "far" | "half" | "phase";

/** The voxels of a slice as an image: `w` × `h`, `at(u, v)` the voxel's code, row 0 on top. */
function image(w: number, h: number, at: (u: number, v: number) => number): string {
  if (typeof document === "undefined") return "";
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d");
  if (!ctx) return "";
  for (let v = 0; v < h; v++) {
    for (let u = 0; u < w; u++) {
      const code = at(u, v);
      if (code === 0) continue;
      const count = code & 0xff;
      ctx.fillStyle = code >> 8 > 0 ? MEASURED : VIEW_COLOURS[Math.min(VIEW_COLOURS.length, count) - 1]!;
      ctx.fillRect(u, v, 1, 1);
    }
  }
  return c.toDataURL();
}

/** A camera of the rig in the rig's frame: where it is, and its axis. */
interface Eye {
  id: string;
  at: Vector3;
  axis: Vector3;
  sharp: Sharpness;
}

function eyesOf(analysis: RigAnalysis): Eye[] {
  return analysis.cameras.map((c, i) => {
    const q = new Quaternion(...c.parent_se3_self.rotation);
    return {
      id: c.id,
      at: new Vector3(...c.parent_se3_self.translation),
      axis: new Vector3(0, 0, 1).applyQuaternion(q),
      sharp: analysis.sharp[i] ?? { kind: "everywhere" },
    };
  });
}

function Slice({
  label,
  caption,
  testId,
  view,
  box,
  src,
  onDrag,
  children,
}: {
  label: string;
  caption: string;
  testId: string;
  /** The drawing's extent `[x0, y0, x1, y1]` (m). */
  view: [number, number, number, number];
  /** Where the voxels go `[x0, y0, x1, y1]` (m). */
  box: [number, number, number, number];
  src: string;
  onDrag: (handle: Handle, at: Pt) => void;
  children: (scale: number, grab: (h: Handle) => (e: ReactPointerEvent) => void) => ReactNode;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<Handle | null>(null);
  const toSlice = (e: { clientX: number; clientY: number }): Pt | null => {
    const ctm = svg.current?.getScreenCTM();
    if (!ctm) return null;
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    return [p.x, p.y];
  };
  const grab = (h: Handle) => (e: ReactPointerEvent) => {
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);
    drag.current = h;
  };
  const [x0, y0, x1, y1] = view;
  return (
    <figure className="flex min-h-0 min-w-0 flex-col gap-1">
      <figcaption className="flex items-baseline justify-between gap-2 text-xs">
        <span className="font-medium">{label}</span>
        <span className="truncate text-fg-muted">{caption}</span>
      </figcaption>
      <svg
        ref={svg}
        data-testid={testId}
        role="img"
        aria-label={`${label}: ${caption}`}
        className="min-h-0 w-full flex-1 touch-none rounded border border-line bg-surface-sunken"
        viewBox={`${x0} ${y0} ${x1 - x0} ${y1 - y0}`}
        preserveAspectRatio="xMidYMid meet"
        onPointerMove={(e) => {
          const at = drag.current && toSlice(e);
          if (drag.current && at) onDrag(drag.current, at);
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
      >
        {src && (
          <image
            href={src}
            x={box[0]}
            y={box[1]}
            width={box[2] - box[0]}
            height={box[3] - box[1]}
            preserveAspectRatio="none"
            style={{ imageRendering: "pixelated" }}
          />
        )}
        <rect
          x={box[0]}
          y={box[1]}
          width={box[2] - box[0]}
          height={box[3] - box[1]}
          fill="none"
          className="stroke-line-strong"
          vectorEffect="non-scaling-stroke"
        />
        {children(x1 - x0, grab)}
      </svg>
    </figure>
  );
}

/** A handle: a shape `r` across at `at`. */
function Knob({
  at,
  r,
  shape,
  label,
  name,
  grab,
}: {
  at: Pt;
  r: number;
  shape: "circle" | "square" | "diamond";
  label: string;
  name: Handle;
  grab: (h: Handle) => (e: ReactPointerEvent) => void;
}) {
  const common = {
    fill: "var(--color-surface)",
    stroke: "var(--color-signal)",
    strokeWidth: 2,
    vectorEffect: "non-scaling-stroke" as const,
    className: "cursor-grab",
    onPointerDown: grab(name),
    role: "slider",
    "aria-label": label,
    "data-handle": name,
  };
  const [x, y] = at;
  if (shape === "circle") return <circle cx={x} cy={y} r={r} {...common} />;
  if (shape === "square") return <rect x={x - r} y={y - r} width={2 * r} height={2 * r} {...common} />;
  return <polygon points={`${x - r},${y} ${x},${y - r} ${x + r},${y} ${x},${y + r}`} {...common} />;
}

const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));

/** `p` with handle `h` dragged to `at` on the side slice (`[x, z]`). */
function dragSide(p: RigParams, h: Handle, [x, z]: Pt): RigParams {
  switch (h) {
    case "working":
      return withWorkingDistance(p, Math.max(0.02, z));
    case "radius":
      return { ...p, radius_m: Math.max(0.005, Math.abs(x)) };
    case "tilt":
      return z > 0.005 ? { ...p, tilt_rad: clamp(Math.atan2(p.radius_m - x, z), -1.4, 1.4) } : p;
    case "near":
      return { ...p, box: { ...p.box, near_m: clamp(z, 0.01, p.box.far_m - 0.005) } };
    case "far":
      return { ...p, box: { ...p.box, far_m: Math.max(z, p.box.near_m + 0.005) } };
    case "half":
      return { ...p, box: { ...p.box, half_m: Math.max(0.005, Math.abs(x)) } };
    default:
      return p;
  }
}

/** `p` with handle `h` dragged to `at` on the top slice (`[x, −y]`). */
function dragTop(p: RigParams, h: Handle, [x, my]: Pt): RigParams {
  if (h !== "phase") return p;
  const radius_m = Math.max(0.005, Math.hypot(x, my));
  return p.arrangement === "ring" ? { ...p, radius_m, phase_rad: Math.atan2(-my, x) } : { ...p, radius_m };
}

export function SliceMaps({
  analysis,
  params,
  onChange,
}: {
  analysis: RigAnalysis;
  params: RigParams;
  onChange?: ((p: RigParams) => void) | undefined;
}) {
  const { grid, coverage, layout } = analysis;
  const [nx, ny, nz] = grid.n;
  const [gx0, gy0, gz0] = grid.min;
  const [gx1, gy1, gz1] = grid.max;
  const dz = (gz1 - gz0) / nz;
  const kWork = Math.min(nz - 1, Math.max(0, Math.floor((layout.working_distance_m - gz0) / dz)));
  const jMid = Math.floor(ny / 2);
  const zTop = gz0 + (kWork + 0.5) * dz;
  const [side, top] = useMemo(() => {
    const code = (i: number, j: number, k: number) => {
      const index = i + nx * (j + ny * k);
      return coverage.views[index]! | (coverage.lit[index]! << 8);
    };
    return [image(nx, nz, (u, v) => code(u, jMid, v)), image(nx, ny, (u, v) => code(u, ny - 1 - v, kWork))];
  }, [coverage, nx, ny, nz, jMid, kWork]);
  const eyes = useMemo(() => eyesOf(analysis), [analysis]);

  const rho = params.arrangement === "stereo" ? params.radius_m : layout.radius_m;
  const reach = Math.max(gx1, -gx0, gy1, -gy0, rho) * 1.15 + 0.01;
  const zMin = Math.min(0, gz0) - 0.08 * (gz1 - Math.min(0, gz0));
  const zMax = gz1 + 0.05 * (gz1 - zMin);
  const tilt = tiltOf(params);
  const wd = params.working_distance_m;
  const mm = (m: number) => `${(m * 1000).toFixed(0)} mm`;
  const edit = onChange ?? null;
  const phase = params.arrangement === "ring" ? params.phase_rad : 0;

  return (
    <div className="grid h-full min-h-0 grid-cols-2 gap-3 p-3" data-testid="rig-slices">
      <Slice
        label="Side"
        caption={`x across, ${mm(gz0)} to ${mm(gz1)} down the axis`}
        testId="rig-slice-side"
        view={[-reach, zMin, reach, zMax]}
        box={[gx0, gz0, gx1, gz1]}
        src={side}
        onDrag={(h, at) => edit?.(dragSide(params, h, at))}
      >
        {(scale, grab) => {
          const r = scale * 0.012;
          return (
            <>
              {/* The rig axis and the working point. */}
              <line
                x1={0}
                y1={zMin}
                x2={0}
                y2={zMax}
                stroke="var(--color-line-strong)"
                strokeDasharray="4 4"
                vectorEffect="non-scaling-stroke"
              />
              {eyes.map((e, i) => {
                const inPlane = Math.abs(e.at.y) < 1e-6;
                const end = e.at.clone().addScaledVector(e.axis, (zMax - e.at.z) / Math.max(1e-6, e.axis.z));
                const band =
                  e.sharp.kind === "between"
                    ? [
                        e.at.clone().addScaledVector(e.axis, e.sharp.near),
                        e.at.clone().addScaledVector(e.axis, e.sharp.far ?? (zMax - e.at.z) / Math.max(1e-6, e.axis.z)),
                      ]
                    : null;
                return (
                  <g key={e.id} opacity={inPlane ? 1 : 0.35} data-camera={i}>
                    <line
                      x1={e.at.x}
                      y1={e.at.z}
                      x2={end.x}
                      y2={end.z}
                      stroke="var(--color-fg-muted)"
                      strokeWidth={1}
                      vectorEffect="non-scaling-stroke"
                    />
                    {band && (
                      <g data-sharp={`${(band[0]!.z * 1000).toFixed(0)}–${(band[1]!.z * 1000).toFixed(0)}`}>
                        <title>{`${e.id}: sharp along its axis`}</title>
                        <line
                          x1={band[0]!.x}
                          y1={band[0]!.z}
                          x2={band[1]!.x}
                          y2={band[1]!.z}
                          stroke="var(--color-fg)"
                          strokeOpacity={0.3}
                          strokeWidth={7}
                          vectorEffect="non-scaling-stroke"
                        />
                        {(["near", "far"] as const).map((which, n) => {
                          // A tick across the axis at each end.
                          const b = band[n]!;
                          const [ax, az] = [e.axis.x, e.axis.z];
                          const len = Math.hypot(ax, az) || 1;
                          const [tx, tz] = [(-az / len) * r * 0.9, (ax / len) * r * 0.9];
                          return (
                            <line
                              key={which}
                              x1={b.x - tx}
                              y1={b.z - tz}
                              x2={b.x + tx}
                              y2={b.z + tz}
                              stroke="var(--color-fg)"
                              strokeWidth={1.5}
                              vectorEffect="non-scaling-stroke"
                            />
                          );
                        })}
                      </g>
                    )}
                    <circle cx={e.at.x} cy={e.at.z} r={r * 0.6} fill="var(--color-fg)" />
                  </g>
                );
              })}
              {edit && (
                <>
                  <Knob at={[0, wd]} r={r} shape="circle" name="working" label="Working distance" grab={grab} />
                  <Knob at={[rho, 0]} r={r} shape="diamond" name="radius" label="Radius ρ" grab={grab} />
                  {!params.converge && (
                    <Knob
                      at={[rho - wd * Math.tan(tilt), wd]}
                      r={r}
                      shape="circle"
                      name="tilt"
                      label="Tilt φ"
                      grab={grab}
                    />
                  )}
                  <Knob at={[gx0, gz0]} r={r * 0.8} shape="square" name="near" label="Box near" grab={grab} />
                  <Knob at={[gx0, gz1]} r={r * 0.8} shape="square" name="far" label="Box far" grab={grab} />
                  <Knob
                    at={[gx1, (gz0 + gz1) / 2]}
                    r={r * 0.8}
                    shape="square"
                    name="half"
                    label="Box half width"
                    grab={grab}
                  />
                </>
              )}
            </>
          );
        }}
      </Slice>
      <Slice
        label="Top"
        caption={`x–y at ${mm(zTop)}`}
        testId="rig-slice-top"
        view={[-reach, -reach, reach, reach]}
        box={[gx0, -gy1, gx1, -gy0]}
        src={top}
        onDrag={(h, at) => edit?.(dragTop(params, h, at))}
      >
        {(scale, grab) => {
          const r = scale * 0.012;
          return (
            <>
              <circle cx={0} cy={0} r={r * 0.4} fill="var(--color-fg-muted)" />
              {eyes.map((e) => {
                // Where the camera's axis crosses the slice.
                const t = (zTop - e.at.z) / Math.max(1e-6, e.axis.z);
                const hit = e.at.clone().addScaledVector(e.axis, t);
                return (
                  <g key={e.id}>
                    <line
                      x1={e.at.x}
                      y1={-e.at.y}
                      x2={hit.x}
                      y2={-hit.y}
                      stroke="var(--color-fg-muted)"
                      strokeWidth={1}
                      vectorEffect="non-scaling-stroke"
                    />
                    <circle cx={e.at.x} cy={-e.at.y} r={r * 0.6} fill="var(--color-fg)" />
                  </g>
                );
              })}
              {edit && (
                <Knob
                  at={[rho * Math.cos(phase), -rho * Math.sin(phase)]}
                  r={r}
                  shape="diamond"
                  name="phase"
                  label={params.arrangement === "ring" ? "Radius ρ and phase" : "Radius ρ"}
                  grab={grab}
                />
              )}
            </>
          );
        }}
      </Slice>
    </div>
  );
}

/** The legend: a swatch per view count up to the rig's cameras, and the measured mark. */
export function CoverageLegend({ cameras, pairs }: { cameras: number; pairs: number }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fg-muted">
      {Array.from({ length: Math.min(cameras, VIEW_COLOURS.length) }, (_, i) => (
        <span key={i} className="inline-flex items-center gap-1">
          <span className="inline-block size-2.5 rounded-sm" style={{ background: VIEW_COLOURS[i] }} />
          {i + 1} {i === 0 ? "view" : "views"}
        </span>
      ))}
      {pairs > 0 && (
        <span className="inline-flex items-center gap-1">
          <span className="inline-block size-2.5 rounded-sm" style={{ background: MEASURED }} />
          measured
        </span>
      )}
      <span className="inline-flex items-center gap-1">
        <span className="inline-block h-1.5 w-3 rounded-sm bg-fg opacity-30" />
        sharp along a camera's axis
      </span>
    </div>
  );
}

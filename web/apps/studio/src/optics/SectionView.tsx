import type { HeadSection, SectionLine, SectionPoint } from "@etendue/wasm";
import { Button } from "@vitavision/ui";
import { Maximize, ZoomIn } from "lucide-react";
import { type PointerEvent as ReactPointerEvent, type WheelEvent, useRef, useState } from "react";

import type { HeadParams } from "./design";

/** A view box in metres: `x` is the camera axis `z`, `y` its `s` (camera +y, down). */
interface View {
  x: number;
  y: number;
  w: number;
  h: number;
}

type Handle = "focus" | "aim" | "laser" | "window";

const ASPECT = 0.42;

function fit(points: SectionPoint[], margin = 0.08): View {
  const zs = points.map((p) => p[1]);
  const ss = points.map((p) => p[0]);
  let [x0, x1] = [Math.min(...zs), Math.max(...zs)];
  let [y0, y1] = [Math.min(...ss), Math.max(...ss)];
  const dx = (x1 - x0) * margin + 1e-3;
  const dy = (y1 - y0) * margin + 1e-3;
  [x0, x1, y0, y1] = [x0 - dx, x1 + dx, y0 - dy, y1 + dy];
  // Keep the drawing's aspect: grow the shorter side.
  const w = Math.max(x1 - x0, (y1 - y0) / ASPECT);
  const h = w * ASPECT;
  return { x: (x0 + x1) / 2 - w / 2, y: (y0 + y1) / 2 - h / 2, w, h };
}

/** Everything worth seeing at once: the camera, the laser and the window. */
function fitAll(s: HeadSection): View {
  return fit([s.sensor[0], s.sensor[1], s.laser_origin, ...(s.window ?? []), [0, s.cardinal_points.front_focal]]);
}

/** The camera close up: sensor, lens, pupils. */
function fitLens(s: HeadSection): View {
  const cp = s.cardinal_points;
  const r = Math.max(s.entrance_radius, s.exit_radius, Math.abs(s.sensor[0][0]), Math.abs(s.sensor[1][0]));
  return fit([
    [-r * 1.5, -0.004],
    [r * 1.5, Math.max(cp.front_focal, cp.entrance_pupil) + 0.01],
  ]);
}

/** A long segment along `line`, for lines the view clips. */
function through(line: SectionLine, reach = 10): [SectionPoint, SectionPoint] {
  const [s, z] = line.point;
  const [ds, dz] = line.direction;
  return [
    [s - reach * ds, z - reach * dz],
    [s + reach * ds, z + reach * dz],
  ];
}

const seg = (a: SectionPoint, b: SectionPoint) => ({ x1: a[1], y1: a[0], x2: b[1], y2: b[0] });
const poly = (points: SectionPoint[]) => points.map((p) => `${p[1]},${p[0]}`).join(" ");

/**
 * The head's Scheimpflug drawing, in the plane through the camera axis perpendicular to the
 * laser plane: sensor and lens (principal points, pupils), the laser and its window, the plane of
 * best focus with the depth of field, the field, and traced rays. Handles move the focus, the
 * aim point, the laser and the window's depth; the wheel zooms, dragging the background pans.
 */
export function SectionView({
  section,
  params,
  onChange,
  onCommit,
}: {
  section: HeadSection;
  params: HeadParams;
  onChange: (p: HeadParams) => void;
  /** A drag ended. */
  onCommit?: () => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const [view, setView] = useState<View | null>(null);
  const v = view ?? fitAll(section);
  const drag = useRef<{ handle: Handle | "pan"; start: SectionPoint; view: View } | null>(null);
  const k = v.w; // one unit of "size" on screen, in metres
  const cp = section.cardinal_points;
  const sAxisY = section.s_axis[1] >= 0 ? 1 : -1;

  const toWorld = (e: { clientX: number; clientY: number }): SectionPoint => {
    const el = svg.current;
    const ctm = el?.getScreenCTM();
    if (!el || !ctm) return [0, 0];
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    return [p.y, p.x];
  };

  const begin = (handle: Handle | "pan") => (e: ReactPointerEvent) => {
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);
    drag.current = { handle, start: toWorld(e), view: v };
  };
  const move = (e: ReactPointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const [s, z] = toWorld(e);
    switch (d.handle) {
      case "pan": {
        // Moving the view moves the pointer's world position: measure against the start view.
        const ctm = svg.current?.getScreenCTM();
        if (!ctm) return;
        const p0 = new DOMPoint(0, 0).matrixTransform(ctm.inverse());
        const p1 = new DOMPoint(e.movementX, e.movementY).matrixTransform(ctm.inverse());
        setView({ ...v, x: v.x - (p1.x - p0.x), y: v.y - (p1.y - p0.y) });
        break;
      }
      case "focus":
        if (z > 0.02) onChange({ ...params, focus_distance_m: z });
        break;
      case "aim":
        if (z > params.height_m + 0.01) onChange({ ...params, aim_m: z });
        break;
      case "laser":
        onChange({ ...params, baseline_m: Math.max(0.005, -s * sAxisY), height_m: Math.min(z, params.aim_m - 0.01) });
        break;
      case "window": {
        // The window's depth: twice the distance from its centre (the aim point) along the ray.
        const centre: SectionPoint = [0, params.aim_m];
        const depth = 2 * Math.hypot(s - centre[0], z - centre[1]);
        onChange({ ...params, depth_m: Math.max(0.002, depth) });
        break;
      }
    }
  };
  const end = () => {
    if (drag.current && drag.current.handle !== "pan") onCommit?.();
    drag.current = null;
  };
  const wheel = (e: WheelEvent) => {
    const [s, z] = toWorld(e);
    const f = Math.exp(e.deltaY * 0.0015);
    setView({ x: z - (z - v.x) * f, y: s - (s - v.y) * f, w: v.w * f, h: v.h * f });
  };

  const handle = (
    name: Handle,
    at: SectionPoint,
    shape: "circle" | "diamond" | "square",
    tone: string,
    label: string,
  ) => {
    const r = k * 0.008;
    const [cx, cy] = [at[1], at[0]];
    const common = {
      fill: "var(--color-surface)",
      stroke: tone,
      strokeWidth: 2,
      vectorEffect: "non-scaling-stroke" as const,
      className: "cursor-grab",
      onPointerDown: begin(name),
      role: "slider",
      "aria-label": label,
      "data-handle": name,
    };
    if (shape === "circle") return <circle cx={cx} cy={cy} r={r} {...common} />;
    if (shape === "square") return <rect x={cx - r} y={cy - r} width={2 * r} height={2 * r} {...common} />;
    return <polygon points={`${cx - r},${cy} ${cx},${cy - r} ${cx + r},${cy} ${cx},${cy + r}`} {...common} />;
  };

  const thin = { vectorEffect: "non-scaling-stroke" as const, strokeWidth: 1 };
  const label = (at: SectionPoint, text: string, tone = "var(--color-fg-muted)", dy = 0) => (
    <text x={at[1]} y={at[0] + dy} fontSize={k * 0.016} fill={tone} textAnchor="middle" className="select-none">
      {text}
    </text>
  );
  const [lensTop, lensBottom] = [-Math.max(section.entrance_radius, section.exit_radius) * 1.6, 0];
  const lensR = -lensTop;
  const lensZ: [number, number] = [
    Math.min(cp.rear_principal, cp.front_principal, cp.exit_pupil, cp.entrance_pupil) - lensR * 0.3,
    Math.max(cp.rear_principal, cp.front_principal, cp.exit_pupil, cp.entrance_pupil) + lensR * 0.3,
  ];
  const dof = section.depth_of_field;

  return (
    <div className="relative h-full min-h-0 w-full">
      <div className="absolute top-2 right-2 z-10 flex gap-1">
        <Button
          size="sm"
          variant="ghost"
          icon={<Maximize className="size-3.5" />}
          onClick={() => setView(fitAll(section))}
        >
          Fit
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<ZoomIn className="size-3.5" />}
          onClick={() => setView(fitLens(section))}
        >
          Lens
        </Button>
      </div>
      <svg
        ref={svg}
        viewBox={`${v.x} ${v.y} ${v.w} ${v.h}`}
        preserveAspectRatio="xMidYMid meet"
        className="h-full w-full touch-none select-none"
        role="img"
        aria-label="Scheimpflug section of the head"
        data-testid="section-view"
        onPointerDown={begin("pan")}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
        onWheel={wheel}
      >
        {/* The camera axis. */}
        <line x1={-10} y1={0} x2={10} y2={0} stroke="var(--color-line-strong)" strokeDasharray="4 4" {...thin} />
        {/* Depth of field: between the near and far limits. */}
        {dof && dof[0].length > 1 && dof[1].length > 1 && (
          <polygon points={poly([...dof[0], ...[...dof[1]].reverse()])} fill="var(--color-normal)" fillOpacity={0.1} />
        )}
        {dof &&
          (
            [
              ["near", dof[0]],
              ["far", dof[1]],
            ] as const
          ).map(([side, line]) => (
            <polyline
              key={side}
              points={poly(line)}
              fill="none"
              stroke="var(--color-normal)"
              strokeOpacity={0.6}
              {...thin}
            />
          ))}
        {/* The field. */}
        {(
          [
            ["first", section.field[0]],
            ["last", section.field[1]],
          ] as const
        ).map(([end, f]) => (
          <line key={end} {...seg(f.point, through(f)[1])} stroke="var(--color-line-strong)" {...thin} />
        ))}
        {/* The plane of best focus. */}
        <line
          {...seg(...through(section.focus))}
          stroke="var(--color-normal)"
          strokeDasharray="8 4"
          {...thin}
          strokeWidth={1.5}
        />
        {/* The laser: its plane's trace, and the window on it. */}
        <line {...seg(...through(section.laser_trace))} stroke="var(--color-defect)" strokeOpacity={0.5} {...thin} />
        {section.window && (
          <line
            {...seg(...section.window)}
            stroke="var(--color-defect)"
            strokeWidth={5}
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
        )}
        {/* Traced rays: object → entrance pupil, exit pupil → sensor. */}
        {section.rays.map((r) => {
          const tone = r.kind === "chief" ? "var(--color-signal)" : "var(--color-warn)";
          return (
            <g
              key={`${r.kind}:${r.object.join(",")}:${r.entrance.join(",")}`}
              stroke={tone}
              fill="none"
              strokeOpacity={0.85}
            >
              <polyline points={poly([r.object, r.entrance])} {...thin} />
              <polyline points={poly([r.entrance, r.exit])} {...thin} strokeDasharray="2 3" />
              <polyline points={poly([r.exit, r.on_sensor])} {...thin} />
            </g>
          );
        })}
        {/* The body behind the image plane, and the lens barrel. */}
        <rect x={-0.03} y={-0.025} width={0.03} height={0.05} fill="var(--color-line)" fillOpacity={0.5} />
        <rect
          x={lensZ[0]}
          y={lensTop}
          width={lensZ[1] - lensZ[0]}
          height={lensR * 2}
          fill="none"
          stroke="var(--color-fg-muted)"
          rx={lensR * 0.15}
          {...thin}
        />
        {/* Principal planes and pupils. */}
        {(
          [
            ["rear", cp.rear_principal, "3 3"],
            ["front", cp.front_principal, "none"],
          ] as const
        ).map(([name, z, dash]) => (
          <line
            key={name}
            x1={z}
            y1={lensTop}
            x2={z}
            y2={lensR}
            stroke="var(--color-signal)"
            strokeDasharray={dash}
            {...thin}
          />
        ))}
        {(
          [
            ["entrance", cp.entrance_pupil, section.entrance_radius],
            ["exit", cp.exit_pupil, section.exit_radius],
          ] as const
        ).map(([name, z, r]) => (
          <g key={name} stroke="var(--color-fg)" strokeWidth={2} vectorEffect="non-scaling-stroke">
            <line x1={z} y1={lensTop} x2={z} y2={-r} vectorEffect="non-scaling-stroke" />
            <line x1={z} y1={r} x2={z} y2={lensR} vectorEffect="non-scaling-stroke" />
          </g>
        ))}
        {/* The sensor. */}
        <line {...seg(...section.sensor)} stroke="var(--color-fg)" strokeWidth={4} vectorEffect="non-scaling-stroke" />
        {/* The hinge rule: the sensor at H′ and the focus plane at H share a height. */}
        {section.sensor_at_rear_principal && section.focus_at_front_principal && (
          <g fill="var(--color-normal)">
            <circle cx={section.sensor_at_rear_principal[1]} cy={section.sensor_at_rear_principal[0]} r={k * 0.003} />
            <circle cx={section.focus_at_front_principal[1]} cy={section.focus_at_front_principal[0]} r={k * 0.003} />
          </g>
        )}
        {label([lensBottom + lensR * 1.25, cp.rear_principal], "H′")}
        {label([lensBottom + lensR * 1.25, cp.front_principal], "H")}
        {label([lensTop - lensR * 0.15, cp.entrance_pupil], "E", "var(--color-fg)")}
        {label([lensTop - lensR * 0.15, cp.exit_pupil], "E′", "var(--color-fg)")}
        {label([section.sensor[1][0] + lensR * 0.4, 0], "S")}
        {/* The focus rides below the axis (the aim point is on it), with a tick to its point. */}
        <line
          x1={params.focus_distance_m}
          y1={0}
          x2={params.focus_distance_m}
          y2={k * 0.03}
          stroke="var(--color-normal)"
          {...thin}
        />
        {handle("focus", [k * 0.03, params.focus_distance_m], "diamond", "var(--color-normal)", "Focus distance")}
        {handle("aim", [0, params.aim_m], "circle", "var(--color-defect)", "Laser aim point")}
        {handle("laser", section.laser_origin, "square", "var(--color-defect)", "Laser origin")}
        {section.window && handle("window", section.window[1], "circle", "var(--color-defect)", "Window depth")}
      </svg>
    </div>
  );
}

import type { CameraSpec, Candidate, EtendueScene, Iso3Schema } from "@etendue/wasm";
import { type FrameTreeRuntime, type LightShapeLike, imageBorderPixels } from "@vitavision/three";
import {
  AtFrame,
  CameraFrustum,
  FrameAxes,
  FrameTree,
  LaserFan,
  LightGizmo,
  Robot,
  SceneCanvas,
  invalidateScene,
  useFrameTree,
} from "@vitavision/three-react";
import { SegmentedControl, ToggleChip } from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { useEffect, useMemo, useState } from "react";

import type { LoadedRobot } from "../io/load";
import type { Baked } from "../kernel/etendue";
import { cachedMeshLoader } from "../kernel/meshCache";
import { type Bodies, type PoseOverride, type PoseSource, showOverrides } from "../kernel/posed";
import { Gizmo, type GizmoOptions } from "../scene/Gizmo";
import { CandidateGlyphs } from "../scene/CandidateGlyphs";
import { type EstimateDrawing, Estimates } from "../scene/Estimates";
import { TargetSurface } from "../scene/TargetSurface";
import { TcpPath } from "../scene/TcpPath";

/** Per kernel session: each camera's border rays and the model they were computed for. */
const rayCache = new WeakMap<EtendueScene, Map<string, { key: string; rays: Float64Array }>>();

/**
 * Border rays of every camera, from the kernel's back-projection (distortion included). A
 * camera's rays are computed again only when its model or image size changes, not when it or
 * anything else moves.
 */
function useBorderRays(session: EtendueScene, cameras: readonly CameraSpec[]): Map<string, Float64Array> {
  return useMemo(() => {
    let cache = rayCache.get(session);
    if (!cache) rayCache.set(session, (cache = new Map<string, { key: string; rays: Float64Array }>()));
    const rays = new Map<string, Float64Array>();
    for (const c of cameras) {
      const key = JSON.stringify([c.params, c.resolution]);
      let hit = cache.get(c.id);
      if (hit?.key !== key) {
        hit = { key, rays: session.backprojectPixels(c.id, imageBorderPixels(c.resolution[0], c.resolution[1], 8)) };
        cache.set(c.id, hit);
      }
      rays.set(c.id, hit.rays);
    }
    return rays;
  }, [session, cameras]);
}

function RobotVisual({ robot, tcp }: { robot: LoadedRobot; tcp: string }) {
  const axes = useMemo(() => [tcp], [tcp]);
  return (
    <Robot
      id={robot.id}
      visuals={robot.manifest.visuals}
      resolve={robot.meshUrl}
      loader={cachedMeshLoader}
      axes={axes}
    />
  );
}

/** Draw `overrides` (an edit in progress) over the frame tree's baked poses. */
function PoseOverrides({ overrides, bodies }: { overrides: readonly PoseOverride[]; bodies: Bodies }) {
  const runtime = useFrameTree();
  useEffect(() => {
    showOverrides(runtime, overrides, bodies);
    // Sensor images draw on their own schedule: wake them.
    invalidateScene(runtime);
  }, [runtime, overrides, bodies]);
  return null;
}

/** The selected entity's handle in the viewport: where it is drawn and what it moves. */
export interface GizmoTarget {
  /** The frame the handle sits on. */
  frame: string;
  /** The parent frame of the placement it edits. */
  parent: string;
  /** The placement as shown. */
  placement: Iso3Schema;
  onPreview: (placement: Iso3Schema) => void;
  onCommit: () => void;
  onCancel: () => void;
}

/** Whether a key event belongs to a text field. */
function inField(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return t !== null && (t.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(t.tagName));
}

/** The gizmo's mode, space and snap, with W (move), E (rotate), Q (local/world) outside fields. */
function GizmoBar({ options, onChange }: { options: GizmoOptions; onChange: (o: GizmoOptions) => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || inField(e)) return;
      const key = e.key.toLowerCase();
      if (key === "w") onChange({ ...options, mode: "translate" });
      else if (key === "e") onChange({ ...options, mode: "rotate" });
      else if (key === "q") onChange({ ...options, space: options.space === "local" ? "world" : "local" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [options, onChange]);
  return (
    <div className="absolute top-2 left-2 z-10 flex items-center gap-1.5 rounded-panel border border-line bg-surface/90 p-1 shadow-sm">
      <SegmentedControl
        aria-label="Gizmo mode"
        value={options.mode}
        options={[
          { value: "translate", label: "Move (W)" },
          { value: "rotate", label: "Rotate (E)" },
        ]}
        onValueChange={(v) => (v === "translate" || v === "rotate") && onChange({ ...options, mode: v })}
      />
      <ToggleChip
        checked={options.space === "local"}
        onCheckedChange={(local) => onChange({ ...options, space: local ? "local" : "world" })}
        title="Gizmo axes: the frame's own (on) or the world's (off); Q toggles"
      >
        Local
      </ToggleChip>
      <ToggleChip
        checked={options.snap}
        onCheckedChange={(snap) => onChange({ ...options, snap })}
        title="Snap to 10 mm and 5°"
      >
        Snap
      </ToggleChip>
    </div>
  );
}

const NONE: readonly PoseOverride[] = [];
const NO_BODIES: Bodies = new Map();
const NO_CAMERAS: readonly CameraSpec[] = [];

/** The 3D scene: robots, rigs, cameras, lasers, lights, targets, posed at the playhead. */
export function Viewport({
  current,
  playhead,
  selected,
  onSelect,
  onRuntime,
  candidates,
  overrides = NONE,
  bodies = NO_BODIES,
  poses = null,
  gizmo = null,
  paths,
  estimate = null,
}: {
  current: Baked;
  playhead: Playhead;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onRuntime: (runtime: FrameTreeRuntime) => void;
  /** Generated candidate poses to show, if any. */
  candidates?: readonly Candidate[] | null | undefined;
  /** Moves of an edit in progress, drawn over the bake. */
  overrides?: readonly PoseOverride[] | undefined;
  /** Cameras with optics: their bodies in their (baked) projection frames. */
  bodies?: Bodies | undefined;
  /** World poses of bodies, the edit in progress included (for the gizmo). */
  poses?: PoseSource | null | undefined;
  /** The selection's handle, when it can be moved. */
  gizmo?: GizmoTarget | null | undefined;
  /** Robot TCP paths to draw (the Motion workspace). */
  paths?: { path: Float32Array; captures: Float32Array }[] | undefined;
  /** A calibration's estimate to draw beside the truth (the Calibrate workspace). */
  estimate?: EstimateDrawing | null | undefined;
}) {
  const [options, setOptions] = useState<GizmoOptions>({ mode: "translate", space: "local", snap: false });
  const { loaded, session, baked } = current;
  const { scene } = loaded;
  const rays = useBorderRays(session, scene.cameras ?? NO_CAMERAS);
  const selectedFrame = selected !== null && baked.frames.includes(selected) ? selected : null;

  return (
    <div className="relative h-full w-full">
      {gizmo && poses && <GizmoBar options={options} onChange={setOptions} />}
      <SceneCanvas
        className="h-full w-full"
        onPointerMissed={() => onSelect(null)}
        label="Scene viewport"
        // A tab stop: arrows orbit, +/− zoom, 0 resets (book: The studio).
        keyboard
      >
        <FrameTree baked={baked} playhead={playhead} onRuntime={onRuntime}>
          <PoseOverrides overrides={overrides} bodies={bodies} />
          {loaded.robots.map((r) => (
            <RobotVisual key={r.id} robot={r} tcp={r.manifest.tcp_link} />
          ))}
          {[...(scene.frames ?? []), ...(scene.rigs ?? []), ...(scene.parts ?? [])].map((f) => (
            <AtFrame key={f.id} name={f.id}>
              <FrameAxes size={0.04} />
            </AtFrame>
          ))}
          {(scene.cameras ?? []).map((c) => (
            <AtFrame key={c.id} name={c.id}>
              <CameraFrustum
                borderRays={rays.get(c.id)!}
                depth={0.12}
                active={selected === c.id}
                onSelect={() => onSelect(c.id)}
              />
            </AtFrame>
          ))}
          {(scene.lasers ?? []).map((l) => (
            <AtFrame key={l.id} name={l.id}>
              <LaserFan
                halfAngle={l.fan_half_angle}
                length={l.fan_length}
                active={selected === l.id}
                onSelect={() => onSelect(l.id)}
              />
            </AtFrame>
          ))}
          {(scene.lights ?? []).map((l) => (
            <AtFrame key={l.id} name={l.id}>
              <LightGizmo shape={l.shape as LightShapeLike} onSelect={() => onSelect(l.id)} />
            </AtFrame>
          ))}
          {(scene.targets ?? []).map((t) => (
            <AtFrame key={t.id} name={t.id}>
              <TargetSurface session={session} target={t} onSelect={() => onSelect(t.id)} />
              {selected === t.id && <FrameAxes size={0.06} />}
            </AtFrame>
          ))}
          {candidates && candidates.length > 0 && (
            <AtFrame name="world">
              <CandidateGlyphs candidates={candidates} />
            </AtFrame>
          )}
          {estimate && <Estimates drawing={estimate} rays={rays} />}
          {selectedFrame && (
            <AtFrame name={selectedFrame}>
              <FrameAxes size={0.1} />
            </AtFrame>
          )}
        </FrameTree>
        {paths?.map((p, i) => (
          // A path per robot, in scene order.
          // eslint-disable-next-line react/no-array-index-key
          <TcpPath key={i} path={p.path} captures={p.captures} />
        ))}
        {gizmo && poses && (
          <Gizmo
            frame={gizmo.frame}
            parent={gizmo.parent}
            placement={gizmo.placement}
            poses={poses}
            options={options}
            onPreview={gizmo.onPreview}
            onCommit={gizmo.onCommit}
            onCancel={gizmo.onCancel}
          />
        )}
      </SceneCanvas>
    </div>
  );
}

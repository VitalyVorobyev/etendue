import type { CameraSpec, Candidate, EtendueScene } from "@etendue/wasm";
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
import type { Playhead } from "@vitavision/workbench";
import { useEffect, useMemo } from "react";

import type { LoadedRobot } from "../io/load";
import type { Baked } from "../kernel/etendue";
import { cachedMeshLoader } from "../kernel/meshCache";
import { type PoseOverride, showOverrides } from "../kernel/posed";
import { CandidateGlyphs } from "../scene/CandidateGlyphs";
import { TargetSurface } from "../scene/TargetSurface";

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
function PoseOverrides({ overrides }: { overrides: readonly PoseOverride[] }) {
  const runtime = useFrameTree();
  useEffect(() => {
    showOverrides(runtime, overrides);
    // Sensor images draw on their own schedule: wake them.
    invalidateScene(runtime);
  }, [runtime, overrides]);
  return null;
}

const NONE: readonly PoseOverride[] = [];
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
}) {
  const { loaded, session, baked } = current;
  const { scene } = loaded;
  const rays = useBorderRays(session, scene.cameras ?? NO_CAMERAS);
  const selectedFrame = selected !== null && baked.frames.includes(selected) ? selected : null;

  return (
    <SceneCanvas
      className="h-full w-full"
      onPointerMissed={() => onSelect(null)}
      label="Scene viewport"
      // A tab stop: arrows orbit, +/− zoom, 0 resets (book: The studio).
      keyboard
    >
      <FrameTree baked={baked} playhead={playhead} onRuntime={onRuntime}>
        <PoseOverrides overrides={overrides} />
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
        {selectedFrame && (
          <AtFrame name={selectedFrame}>
            <FrameAxes size={0.1} />
          </AtFrame>
        )}
      </FrameTree>
    </SceneCanvas>
  );
}

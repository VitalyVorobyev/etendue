import type { Candidate, EtendueScene, SceneSpec } from "@etendue/wasm";
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
} from "@vitavision/three-react";
import type { Playhead } from "@vitavision/workbench";
import { useMemo } from "react";

import type { LoadedRobot } from "../io/load";
import type { Baked } from "../kernel/etendue";
import { CandidateGlyphs } from "../scene/CandidateGlyphs";
import { TargetSurface } from "../scene/TargetSurface";

/** Border rays of every camera, from the kernel's back-projection (distortion included). */
function useBorderRays(session: EtendueScene, scene: SceneSpec): Map<string, Float64Array> {
  return useMemo(
    () =>
      new Map(
        (scene.cameras ?? []).map((c) => [
          c.id,
          session.backprojectPixels(c.id, imageBorderPixels(c.resolution[0], c.resolution[1], 8)),
        ]),
      ),
    [session, scene],
  );
}

function RobotVisual({ robot, tcp }: { robot: LoadedRobot; tcp: string }) {
  const axes = useMemo(() => [tcp], [tcp]);
  return <Robot id={robot.id} visuals={robot.manifest.visuals} resolve={robot.meshUrl} axes={axes} />;
}

/** The 3D scene: robots, rigs, cameras, lasers, lights, targets, posed at the playhead. */
export function Viewport({
  current,
  playhead,
  selected,
  onSelect,
  onRuntime,
  candidates,
}: {
  current: Baked;
  playhead: Playhead;
  selected: string | null;
  onSelect: (id: string | null) => void;
  onRuntime: (runtime: FrameTreeRuntime) => void;
  /** Generated candidate poses to show, if any. */
  candidates?: readonly Candidate[] | null | undefined;
}) {
  const { loaded, session, baked } = current;
  const { scene } = loaded;
  const rays = useBorderRays(session, scene);
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

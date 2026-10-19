import type { CameraSpec, EtendueScene, LaserSpec, TargetSpec } from "@etendue/wasm";
import type { FrameTreeRuntime } from "@vitavision/three";
import { Button, Empty } from "@vitavision/ui";
import type { Playhead } from "@vitavision/workbench";
import { LayoutGrid, Maximize2 } from "lucide-react";
import { useState } from "react";

import type { PoseSource } from "../kernel/posed";
import { CameraView } from "./CameraView";

/** Grid columns for `n` tiles: one row up to three cameras, then two rows. */
function columns(n: number): number {
  if (n <= 3) return Math.max(1, n);
  return Math.ceil(n / 2);
}

/**
 * Every camera's image at the playhead, side by side, or one of them alone. The images are the
 * main surface of the Images workspace; the 3D view shrinks to an inset beside them.
 */
export function CameraGrid({
  session,
  runtime,
  poses,
  cameras,
  targets,
  lasers,
  playhead,
  renderedAt,
  focus,
  onFocus,
}: {
  session: EtendueScene;
  runtime: FrameTreeRuntime | null;
  /** Where poses are read: `runtime`, or it with an edit in progress drawn over it. */
  poses?: PoseSource | null | undefined;
  cameras: readonly CameraSpec[];
  targets: readonly TargetSpec[];
  lasers: readonly LaserSpec[];
  playhead: Playhead;
  renderedAt?: ((camera: string, k: number) => string | null) | undefined;
  /** The camera shown alone, or `null` for all. */
  focus: string | null;
  onFocus: (camera: string | null) => void;
}) {
  const [hover, setHover] = useState<string | null>(null);
  if (cameras.length === 0) return <Empty>The scene has no cameras.</Empty>;
  if (!runtime) return null;
  const shown = focus !== null && cameras.some((c) => c.id === focus) ? cameras.filter((c) => c.id === focus) : cameras;
  const cols = columns(shown.length);
  return (
    <div
      className="grid h-full min-h-0 gap-2 p-2"
      style={{
        gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
        gridTemplateRows: `repeat(${Math.ceil(shown.length / cols)}, minmax(0, 1fr))`,
      }}
      data-testid="camera-grid"
    >
      {shown.map((camera) => (
        <section
          key={camera.id}
          aria-label={camera.id}
          className="flex min-h-0 min-w-0 flex-col rounded-md border border-line bg-surface"
          onPointerEnter={() => setHover(camera.id)}
          onPointerLeave={() => setHover(null)}
        >
          <header className="flex h-8 shrink-0 items-center gap-2 border-b border-line px-2">
            <span className="flex-1 truncate font-mono text-xs">{camera.id}</span>
            <span className="font-mono text-[11px] text-fg-muted">
              {camera.resolution[0]}×{camera.resolution[1]}
            </span>
            {cameras.length > 1 && (hover === camera.id || focus !== null) && (
              <Button
                variant="ghost"
                size="sm"
                icon={focus === null ? <Maximize2 className="size-3.5" /> : <LayoutGrid className="size-3.5" />}
                aria-label={focus === null ? `Show ${camera.id} alone` : "Show every camera"}
                onClick={() => onFocus(focus === null ? camera.id : null)}
              />
            )}
          </header>
          <div className="min-h-0 flex-1 p-1">
            <CameraView
              session={session}
              runtime={runtime}
              poses={poses ?? runtime}
              camera={camera}
              targets={targets}
              lasers={lasers}
              playhead={playhead}
              renderedAt={renderedAt}
            />
          </div>
        </section>
      ))}
    </div>
  );
}

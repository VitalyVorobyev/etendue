import { createPortal, useFrame } from "@react-three/fiber";
import { type BakedScenarioLike, FrameTreeRuntime } from "@vitavision/three";
import { createContext, type ReactNode, use, useEffect, useMemo } from "react";

const RuntimeContext = createContext<FrameTreeRuntime | null>(null);

/** Where playback is: read once per rendered frame, never through React state. */
export interface PlayheadSource {
  /** The sample index to show. */
  get(): number;
}

/** Props of {@link FrameTree}. */
export interface FrameTreeProps {
  /** The baked scenario to show. A new object builds a new runtime. */
  baked: BakedScenarioLike;
  /** The sample to show, read every rendered frame. Omit to stay at sample 0. */
  playhead?: PlayheadSource | undefined;
  /** Called with the runtime once it is built (e.g. to read poses for an inspector). */
  onRuntime?: ((runtime: FrameTreeRuntime) => void) | undefined;
  /** Scene content; place it in frames with {@link AtFrame}. */
  children?: ReactNode;
}

/**
 * Mounts a `FrameTreeRuntime` (`@vitavision/three`) for `baked` and poses it at `playhead.get()` on every
 * rendered frame (`useFrame`), so playback never re-renders React.
 */
export function FrameTree({ baked, playhead, onRuntime, children }: FrameTreeProps) {
  const runtime = useMemo(() => new FrameTreeRuntime(baked), [baked]);
  useEffect(() => {
    onRuntime?.(runtime);
  }, [runtime, onRuntime]);
  useFrame(() => {
    if (playhead) runtime.apply(playhead.get());
  });
  return (
    <RuntimeContext value={runtime}>
      <primitive object={runtime.root} />
      {children}
    </RuntimeContext>
  );
}

/** The runtime of the enclosing {@link FrameTree}. */
export function useFrameTree(): FrameTreeRuntime {
  const runtime = use(RuntimeContext);
  if (!runtime) throw new Error("useFrameTree must be used inside <FrameTree>");
  return runtime;
}

/** Props of {@link AtFrame}. */
export interface AtFrameProps {
  /** Frame name, e.g. `"cam_left"` or `"ur5e/tool0"`. */
  name: string;
  /** Content, expressed in that frame. */
  children?: ReactNode;
}

/**
 * Renders `children` in frame `name` of the enclosing {@link FrameTree}: they move with it.
 * Renders nothing for an unknown frame.
 */
export function AtFrame({ name, children }: AtFrameProps) {
  const frame = useFrameTree().frame(name);
  return frame ? createPortal(children, frame) : null;
}

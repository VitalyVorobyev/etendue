import type { Meta, StoryObj } from "@storybook/react-vite";
import { type FrameTreeRuntime, type MeshLoader, imageBorderPixels } from "@vitavision/three";
import { useMemo } from "react";
import { expect, waitFor } from "storybook/test";
import { BoxGeometry, Mesh, MeshBasicMaterial } from "three";

import { AtFrame, FrameTree, type PlayheadSource } from "./FrameTree";
import { CameraFrustum, FrameAxes, LaserFan, LightGizmo, TargetBoard } from "./gizmos";
import { Robot } from "./Robot";
import { SceneCanvas } from "./SceneCanvas";

const I = { rotation: [0, 0, 0, 1], translation: [0, 0, 0] };

/** A camera orbiting a board at 0.5 m, looking down at it: 60 samples, 0.05 s apart. */
function orbitScenario() {
  const samples = Array.from({ length: 60 }, (_, k) => {
    const a = (2 * Math.PI * k) / 60;
    // Camera at radius 0.4, height 0.5, optical axis (+Z) pointing down (rotation π about X,
    // then yaw a about world Z).
    const half = a / 2;
    const cz = Math.cos(half);
    const sz = Math.sin(half);
    // q = qz(a) · qx(π) = (cz, 0, 0, sz) ⊗ (0 w, 1 x) → [x, y, z, w] = [cz, sz, 0, 0]
    const camera = { rotation: [cz, sz, 0, 0], translation: [0.4 * Math.cos(a), 0.4 * Math.sin(a), 0.5] };
    const link = { rotation: [0, 0, sz, cz], translation: [0, 0, 0.1] };
    return {
      t: k * 0.05,
      world_se3_frame: [I, I, link, camera, I, { rotation: [0, 0, 0, 1], translation: [0.2, 0, 0.6] }],
      ...(k % 20 === 0 ? { capture: { id: `c${k / 20}` } } : {}),
    };
  });
  return {
    dt: 0.05,
    frames: ["world", "arm/base", "arm/link1", "cam", "board", "lamp"],
    samples,
  };
}

/** Pinhole border rays for a 60° × 48° field of view (a test double for `backprojectPixels`). */
function borderRays(): Float64Array {
  const px = imageBorderPixels(1280, 1024, 8);
  const f = 1280 / 2 / Math.tan(Math.PI / 6);
  const rays = new Float64Array((px.length / 2) * 3);
  for (let i = 0; i < px.length / 2; i++) rays.set([(px[2 * i]! - 640) / f, (px[2 * i + 1]! - 512) / f, 1], 3 * i);
  return rays;
}

const boxLoader: MeshLoader = {
  load: (url) =>
    url.includes("missing")
      ? Promise.reject(new Error("404"))
      : Promise.resolve(new Mesh(new BoxGeometry(0.08, 0.08, 0.2), new MeshBasicMaterial())),
};

/** The runtime each rendered story built, by sample — how `play` reaches it. */
const RUNTIMES = new Map<number, FrameTreeRuntime>();

interface CellProps {
  /** Sample shown. */
  sample: number;
}

function Cell({ sample }: CellProps) {
  const baked = useMemo(() => orbitScenario(), []);
  const rays = useMemo(() => borderRays(), []);
  const playhead = useMemo<PlayheadSource>(() => ({ get: () => sample }), [sample]);
  return (
    <div data-testid="cell" data-sample={sample}>
      <SceneCanvas className="h-96 w-full" eye={[1.2, -1, 0.9]} target={[0, 0, 0.2]} label="Robot cell">
        <FrameTree
          baked={baked}
          playhead={playhead}
          onRuntime={(r) => RUNTIMES.set(sample, r)}
        >
          <Robot
            id="arm"
            visuals={[
              { link: "base", mesh: "base.glb" },
              { link: "link1", mesh: "missing.glb" },
            ]}
            resolve={(p) => p}
            loader={boxLoader}
            axes={["base"]}
          />
          <AtFrame name="cam">
            <FrameAxes />
            <CameraFrustum borderRays={rays} depth={0.15} active onSelect={() => undefined} />
          </AtFrame>
          <AtFrame name="board">
            <TargetBoard width={0.25} height={0.175} checker={{ cols: 10, rows: 7 }} onSelect={() => undefined} />
          </AtFrame>
          <AtFrame name="lamp">
            <LightGizmo shape={{ type: "spot", cone_angle: 0.6 }} />
          </AtFrame>
          <AtFrame name="arm/link1">
            <LaserFan halfAngle={0.4} length={0.3} />
          </AtFrame>
          <AtFrame name="no/such/frame">
            <FrameAxes />
          </AtFrame>
        </FrameTree>
      </SceneCanvas>
    </div>
  );
}

const meta = {
  title: "three-react/Robot cell",
  component: Cell,
  args: { sample: 0 },
  argTypes: { sample: { control: { type: "range", min: 0, max: 59, step: 1 } } },
} satisfies Meta<typeof Cell>;

export default meta;
type Story = StoryObj<typeof meta>;

async function renderedRuntime(canvasElement: HTMLElement, sample: number): Promise<FrameTreeRuntime> {
  await waitFor(() => expect(canvasElement.querySelector("canvas")).not.toBeNull());
  await waitFor(() => expect(RUNTIMES.get(sample)).toBeDefined());
  return RUNTIMES.get(sample)!;
}

/** A camera orbiting a checkerboard, a robot with one missing link mesh (drawn as axes), a laser, a spot light. */
export const Default: Story = {
  play: async ({ canvasElement, args }) => {
    const runtime = await renderedRuntime(canvasElement, args.sample);
    await expect(runtime.captures.map((c) => c.id)).toEqual(["c0", "c1", "c2"]);
    await waitFor(() => expect(runtime.current).toBe(0));
    // The robot's box mesh is attached to its base frame.
    await waitFor(() => expect(runtime.frame("arm/base")!.children.length).toBeGreaterThan(0));
  },
};

/** The playhead is read every rendered frame: the runtime follows it without a React update. */
export const Playhead: Story = {
  args: { sample: 30 },
  play: async ({ canvasElement, args }) => {
    const runtime = await renderedRuntime(canvasElement, args.sample);
    await waitFor(() => expect(runtime.current).toBe(30));
    const pose = runtime.pose("cam")!;
    // Half an orbit: the camera is on the −X side.
    await expect(pose.translation[0]).toBeCloseTo(-0.4, 6);
  },
};

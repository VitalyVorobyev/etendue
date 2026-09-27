import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fn, userEvent, waitFor, within } from "storybook/test";

import { DensityProvider } from "@vitavision/ui";

import { PlaybackBar, type PlaybackBarProps, type PlaybackMarker } from "./PlaybackBar";
import { createPlayhead } from "./playhead";
import { usePlaybackClock } from "./usePlayback";

/** Five seconds sampled at 100 Hz: samples 0 … 500. */
const COUNT = 501;
const DT = 0.01;

/** Stop-and-shoot captures along the trajectory. */
const CAPTURES: PlaybackMarker[] = [
  { index: 120, label: "Capture 1" },
  { index: 260, label: "Capture 2" },
  { index: 400, label: "Capture 3", tone: "warn" },
];

/**
 * The bar as an app wires it: the app owns `playing`, `speed` and `loop`, runs the clock, and
 * hands the bar the playhead. `args.playhead` is only a template — each story mounts its own
 * store with the template's `count` and `dt`, so stories never share a position.
 */
/** The story's args: the bar's props, plus the sample the story starts on. */
type TransportArgs = PlaybackBarProps & { start?: number | undefined };

function Transport({ start = 0, ...args }: TransportArgs) {
  const [playhead] = useState(() => {
    const created = createPlayhead(args.playhead.count, args.playhead.dt);
    created.set(start);
    return created;
  });
  const [playing, setPlaying] = useState(args.playing);
  const [speed, setSpeed] = useState(args.speed ?? 1);
  const [loop, setLoop] = useState(args.loop ?? false);
  usePlaybackClock({ playhead, playing, speed, loop, onEnd: () => setPlaying(false) });

  return (
    <div style={{ width: 760 }} className="rounded-panel border border-line">
      <PlaybackBar
        {...args}
        playhead={playhead}
        playing={playing}
        onPlayingChange={(next) => {
          setPlaying(next);
          args.onPlayingChange(next);
        }}
        speed={speed}
        onSpeedChange={(next) => {
          setSpeed(next);
          args.onSpeedChange?.(next);
        }}
        loop={loop}
        onLoopChange={(next) => {
          setLoop(next);
          args.onLoopChange?.(next);
        }}
      />
    </div>
  );
}

const meta = {
  title: "workbench/PlaybackBar",
  component: PlaybackBar,
  parameters: {
    docs: {
      description: {
        component: `The transport for a sampled timeline: jump to start/end, previous/next marker, step, play/pause, a
scrubber over the sample index with markers as ticks, the time \`t = k · dt\` in mono, the speed and loop.

**Use** it with a \`Playhead\` (\`createPlayhead(count, dt)\`) and \`usePlaybackClock\`. \`playing\`, \`speed\`
and \`loop\` are controlled props; the position is the store, which the bar subscribes to itself — during
playback only the bar re-renders, and a 3D view reads \`playhead.get()\` in its own frame loop.

**Don't** use it for a continuous, unsampled quantity (that is a \`Slider\`), or for media with its own
clock (a \`<video>\` has controls).

**Accessibility**: a \`toolbar\` of named icon buttons; the scrubber is a \`slider\` ("Playback position",
←/→ one sample, PageUp/PageDown ten, Home/End). On the buttons ←/→ step one sample; Space toggles playback
except on a button. Marker ticks are pointer shortcuts (\`tabindex=-1\`); the previous/next-marker buttons are
their keyboard route. The loop toggle carries \`aria-pressed\`.`,
      },
    },
  },
  args: {
    playhead: createPlayhead(COUNT, DT),
    playing: false,
    onPlayingChange: fn(),
    onSpeedChange: fn(),
    onLoopChange: fn(),
    markers: CAPTURES,
    onMarkerSelect: fn(),
  },
  render: (args) => <Transport {...args} />,
} satisfies Meta<TransportArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

/** The bar's time readout. */
function readout(canvasElement: HTMLElement): string {
  return canvasElement.querySelector(".font-mono.text-xs")?.textContent ?? "";
}

export const Default: Story = {
  play: async ({ canvas, canvasElement }) => {
    await expect(canvas.getByRole("toolbar", { name: "Playback" })).toBeInTheDocument();
    await expect(readout(canvasElement)).toBe("t = 0.000 s0/500");
    await expect(canvas.getByRole("button", { name: "Jump to start" })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: "Previous marker" })).toBeDisabled();

    await userEvent.click(canvas.getByRole("button", { name: "Step forward" }));
    await expect(readout(canvasElement)).toBe("t = 0.010 s1/500");
    await userEvent.click(canvas.getByRole("button", { name: "Step back" }));
    await expect(readout(canvasElement)).toBe("t = 0.000 s0/500");

    await userEvent.click(canvas.getByRole("button", { name: "Next marker" }));
    await expect(readout(canvasElement)).toContain("1.200 s");
    await userEvent.click(canvas.getByRole("button", { name: "Next marker" }));
    await expect(readout(canvasElement)).toContain("2.600 s");
    await userEvent.click(canvas.getByRole("button", { name: "Previous marker" }));
    await expect(readout(canvasElement)).toContain("1.200 s");

    await userEvent.click(canvas.getByRole("button", { name: "Jump to end" }));
    await expect(readout(canvasElement)).toBe("t = 5.000 s500/500");
    await expect(canvas.getByRole("button", { name: "Step forward" })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: "Next marker" })).toBeDisabled();
    await userEvent.click(canvas.getByRole("button", { name: "Jump to start" }));
    await expect(canvas.getByRole("slider", { name: "Playback position" })).toHaveAttribute("aria-valuenow", "0");
  },
};

export const Playing: Story = {
  play: async ({ canvas, canvasElement, args }) => {
    await userEvent.click(canvas.getByRole("button", { name: "Play" }));
    await expect(args.onPlayingChange).toHaveBeenCalledWith(true);
    await expect(canvas.getByRole("toolbar")).toHaveAttribute("data-playing");
    const slider = canvas.getByRole("slider", { name: "Playback position" });
    await waitFor(() => expect(Number(slider.getAttribute("aria-valuenow"))).toBeGreaterThan(5));
    await userEvent.click(canvas.getByRole("button", { name: "Pause" }));
    const paused = readout(canvasElement);
    await new Promise((resolve) => setTimeout(resolve, 80));
    await expect(readout(canvasElement)).toBe(paused);
  },
};

export const StopsAtEnd: Story = {
  args: { start: 490 },
  play: async ({ canvas, canvasElement }) => {
    await userEvent.click(canvas.getByRole("button", { name: "Play" }));
    // 10 samples at 100 Hz: done in about 0.1 s, and the clock hands `playing` back.
    await waitFor(() => expect(canvas.getByRole("button", { name: "Play" })).toBeInTheDocument());
    await expect(readout(canvasElement)).toBe("t = 5.000 s500/500");
    // Playing again from the end starts over.
    await userEvent.click(canvas.getByRole("button", { name: "Play" }));
    await waitFor(() => expect(readout(canvasElement)).not.toContain("500/500"));
    await userEvent.click(canvas.getByRole("button", { name: "Pause" }));
  },
};

export const Looping: Story = {
  args: { start: 480, loop: true },
  play: async ({ canvas }) => {
    const loop = canvas.getByRole("button", { name: "Loop" });
    await expect(loop).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(canvas.getByRole("button", { name: "Play" }));
    const slider = canvas.getByRole("slider");
    // Past the end it wraps to the start rather than stopping.
    await waitFor(() => expect(Number(slider.getAttribute("aria-valuenow"))).toBeLessThan(100), { timeout: 3000 });
    await expect(canvas.getByRole("button", { name: "Pause" })).toBeInTheDocument();
    await userEvent.click(canvas.getByRole("button", { name: "Pause" }));
    await userEvent.click(loop);
    await expect(loop).toHaveAttribute("aria-pressed", "false");
    await expect(loop).toHaveAttribute("data-state", "off");
  },
};

export const Markers: Story = {
  play: async ({ canvas, canvasElement, args }) => {
    const tick = canvas.getByRole("button", { name: "Capture 3 (4.000 s)" });
    await expect(tick).toHaveAttribute("tabindex", "-1");
    await userEvent.click(tick);
    await expect(readout(canvasElement)).toBe("t = 4.000 s400/500");
    await expect(args.onMarkerSelect).toHaveBeenCalledWith(CAPTURES[2]);
    await expect(tick).toHaveAttribute("data-active");
  },
};

export const Keyboard: Story = {
  play: async ({ canvas, canvasElement, args }) => {
    canvas.getByRole("button", { name: "Step forward" }).focus();
    await userEvent.keyboard("{ArrowRight}{ArrowRight}{ArrowRight}");
    await expect(readout(canvasElement)).toBe("t = 0.030 s3/500");
    await userEvent.keyboard("{ArrowLeft}");
    await expect(readout(canvasElement)).toBe("t = 0.020 s2/500");

    const slider = canvas.getByRole("slider", { name: "Playback position" });
    slider.focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(slider).toHaveAttribute("aria-valuenow", "3");
    await userEvent.keyboard("{End}");
    await expect(slider).toHaveAttribute("aria-valuenow", "500");
    await userEvent.keyboard("{Home}");
    // Space on the scrubber toggles playback.
    await userEvent.keyboard(" ");
    await expect(args.onPlayingChange).toHaveBeenLastCalledWith(true);
    await userEvent.keyboard(" ");
    await expect(args.onPlayingChange).toHaveBeenLastCalledWith(false);
  },
};

export const Speed: Story = {
  play: async ({ canvas, args }) => {
    const trigger = canvas.getByRole("combobox", { name: "Playback speed" });
    await expect(trigger).toHaveTextContent("1×");
    trigger.focus();
    await userEvent.keyboard("{Enter}");
    const listbox = await within(document.body).findByRole("listbox");
    await userEvent.click(within(listbox).getByRole("option", { name: "2×" }));
    await expect(args.onSpeedChange).toHaveBeenCalledWith(2);
    await waitFor(() => expect(trigger).toHaveTextContent("2×"));
  },
};

export const WithoutOptionalControls: Story = {
  render: (args) => (
    // Only the required props: no speed selector, no loop toggle, no markers.
    <PlaybackBar
      playhead={createPlayhead(args.playhead.count, args.playhead.dt)}
      playing={args.playing}
      onPlayingChange={args.onPlayingChange}
    />
  ),
  play: async ({ canvas }) => {
    await expect(canvas.queryByRole("combobox")).toBeNull();
    await expect(canvas.queryByRole("button", { name: "Loop" })).toBeNull();
    await expect(canvas.getByRole("button", { name: "Next marker" })).toBeDisabled();
  },
};

export const Empty: Story = {
  args: { playhead: createPlayhead(0, DT), markers: [] },
  play: async ({ canvas, canvasElement, args }) => {
    await expect(canvas.getByRole("toolbar")).toHaveAttribute("data-empty");
    await expect(canvas.getByRole("button", { name: "Play" })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: "Jump to end" })).toBeDisabled();
    await expect(readout(canvasElement)).toBe("t = 0.000 s–/0");
    canvas.getByRole("toolbar").querySelector<HTMLElement>("[role=slider]")?.focus();
    await userEvent.keyboard(" ");
    await expect(args.onPlayingChange).not.toHaveBeenCalled();
  },
};

export const Compact: Story = {
  render: (args) => (
    <DensityProvider value="compact">
      <Transport {...args} />
    </DensityProvider>
  ),
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("button", { name: "Play" }).className).toContain("size-7");
  },
};

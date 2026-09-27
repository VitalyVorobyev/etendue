import type { Meta, StoryObj } from "@storybook/react-vite";
import { Bot, Camera, Globe, Link2 } from "lucide-react";
import { useState } from "react";
import { expect, fn, userEvent, waitFor } from "storybook/test";

import { DensityProvider, Field, Panel, ReadoutStrip } from "@vitavision/ui";

import { AppShell } from "./AppShell";
import { FileDrop } from "./FileDrop";
import { PlaybackBar } from "./PlaybackBar";
import { createPlayhead } from "./playhead";
import { TreeView } from "./TreeView";
import type { TreeNode } from "./treeModel";
import { usePlaybackClock } from "./usePlayback";

const FRAMES: TreeNode[] = [
  {
    id: "world",
    label: "world",
    icon: <Globe />,
    children: [
      {
        id: "ur5e",
        label: "ur5e",
        icon: <Bot />,
        meta: "robot",
        children: [
          { id: "base", label: "base", icon: <Link2 /> },
          { id: "tool0", label: "tool0", icon: <Link2 />, children: [{ id: "cam0", label: "cam0", icon: <Camera /> }] },
        ],
      },
    ],
  },
];

function Header() {
  return (
    <div className="flex h-11 items-center gap-3 px-3">
      <span className="text-sm font-semibold tracking-tight">etendue studio</span>
      <span className="ml-auto" />
      <FileDrop overlay accept=".json" onFiles={fn()} />
    </div>
  );
}

/** A studio screen: frame tree, viewport, inspector and transport, in a fixed 560 px frame. */
function Studio({ sides = "both", storageKey }: { sides?: "both" | "left" | "none"; storageKey?: string }) {
  const [selected, setSelected] = useState<string | null>("tool0");
  const [playhead] = useState(() => createPlayhead(301, 0.02));
  const [playing, setPlaying] = useState(false);
  usePlaybackClock({ playhead, playing, onEnd: () => setPlaying(false) });

  return (
    <DensityProvider value="compact">
      <div style={{ width: 1200, height: 560 }} className="overflow-hidden rounded-panel border border-line">
        <AppShell
          className="h-full"
          storageKey={storageKey}
          header={<Header />}
          left={
            sides === "none" ? undefined : (
              <TreeView
                nodes={FRAMES}
                aria-label="Cell frames"
                defaultExpandAll
                selectedId={selected}
                onSelect={setSelected}
              />
            )
          }
          main={
            <div style={{ height: "100%" }} className="grid h-full place-items-center bg-canvas text-xs text-fg-subtle">
              3D viewport
            </div>
          }
          right={
            sides === "both" ? (
              <div className="p-2">
                <Panel title="Inspector">
                  <Field label="Frame">
                    <ReadoutStrip items={[{ label: "id", value: selected ?? "—" }]} />
                  </Field>
                </Panel>
              </div>
            ) : undefined
          }
          bottom={<PlaybackBar playhead={playhead} playing={playing} onPlayingChange={setPlaying} />}
        />
      </div>
    </DensityProvider>
  );
}

const meta = {
  title: "workbench/AppShell",
  component: AppShell,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component: `The frame of a studio app: \`header\` across the top, \`left\` | \`main\` | \`right\` with resizable side
panels (each a \`SplitPane\`), \`bottom\` across the foot. Every slot but \`main\` is optional.

**Use** it for a tool whose surfaces are all permanent — navigator, viewport, inspector, timeline — and
pass \`storageKey\` to remember the panel widths. It fills the viewport (\`h-dvh\`); give it \`className="h-full"\`
to embed it.

**Don't** use it for a page that scrolls (a document, a settings form, a dashboard) — \`@vitavision/ui\`'s
\`PageHeader\` and the page's own layout are for those. It has no routing and no menus.

**Accessibility**: landmarks — \`header\`, \`main\`, and the side panels as \`aside\`s named by \`leftLabel\`
("Navigator") and \`rightLabel\` ("Inspector"). Each side panel's divider is a keyboard-operable separator
named after it ("Resize navigator").`,
      },
    },
  },
  args: { main: null },
  render: () => <Studio />,
} satisfies Meta<typeof AppShell>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Studio3Pane: Story = {
  name: "Studio",
  play: async ({ canvas }) => {
    // The shell's header is the first banner (a `Panel`'s header is only one to Testing Library).
    await expect(canvas.getAllByRole("banner")[0]).toHaveTextContent("etendue studio");
    await expect(canvas.getByRole("main")).toHaveTextContent("3D viewport");
    await expect(canvas.getByRole("complementary", { name: "Navigator" })).toBeInTheDocument();
    await expect(canvas.getByRole("complementary", { name: "Inspector" })).toHaveTextContent("tool0");
    await expect(canvas.getByRole("toolbar", { name: "Playback" })).toBeInTheDocument();

    const navigator = canvas.getByRole("separator", { name: "Resize navigator" });
    const inspector = canvas.getByRole("separator", { name: "Resize inspector" });
    await waitFor(() => expect(navigator).toHaveAttribute("aria-valuenow", "280"));
    await expect(inspector).toHaveAttribute("aria-valuenow", "320");
    await expect(navigator).toHaveAttribute("aria-valuemin", "180");

    // The navigator and the inspector share one selection.
    await userEvent.click(canvas.getByRole("treeitem", { name: /cam0/ }));
    await expect(canvas.getByRole("complementary", { name: "Inspector" })).toHaveTextContent("cam0");

    // The inspector's divider is on its left: → shrinks it.
    inspector.focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(inspector).toHaveAttribute("aria-valuenow", "304");
  },
};

export const LeftOnly: Story = {
  render: () => <Studio sides="left" />,
  play: async ({ canvas }) => {
    await expect(canvas.getAllByRole("separator")).toHaveLength(1);
    await expect(canvas.queryByRole("complementary", { name: "Inspector" })).toBeNull();
  },
};

export const MainOnly: Story = {
  render: () => <Studio sides="none" />,
  play: async ({ canvas }) => {
    await expect(canvas.queryByRole("separator")).toBeNull();
    await expect(canvas.queryAllByRole("complementary")).toHaveLength(0);
  },
};

const STORAGE_KEY = "workbench-story-shell";

export const RemembersWidths: Story = {
  beforeEach: () => {
    window.localStorage.setItem(`${STORAGE_KEY}:left`, "220");
    return () => {
      window.localStorage.removeItem(`${STORAGE_KEY}:left`);
      window.localStorage.removeItem(`${STORAGE_KEY}:right`);
    };
  },
  render: () => <Studio storageKey={STORAGE_KEY} />,
  play: async ({ canvas }) => {
    const navigator = canvas.getByRole("separator", { name: "Resize navigator" });
    await waitFor(() => expect(navigator).toHaveAttribute("aria-valuenow", "220"));
    const inspector = canvas.getByRole("separator", { name: "Resize inspector" });
    inspector.focus();
    await userEvent.keyboard("{ArrowLeft}");
    await expect(window.localStorage.getItem(`${STORAGE_KEY}:right`)).toBe("336");
  },
};

/** The bare slots, no header or bottom. */
export const Minimal: Story = {
  args: { main: <p>Main</p>, left: <p>Left</p>, className: "h-64" },
  render: (args) => (
    <div style={{ height: 256 }}>
      <AppShell {...args} />
    </div>
  ),
  play: async ({ canvas }) => {
    await expect(canvas.queryByRole("banner")).toBeNull();
    await expect(canvas.getByRole("main")).toHaveTextContent("Main");
    await expect(canvas.getByRole("complementary", { name: "Navigator" })).toHaveTextContent("Left");
  },
};

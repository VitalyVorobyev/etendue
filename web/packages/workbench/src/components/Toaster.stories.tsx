import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";

import { Button } from "@vitavision/ui";

import { Toaster, type ToasterProps } from "./Toaster";
import { createToastStore, type ToastStore } from "./toastStore";

/**
 * Buttons that raise one toast of each tone. Each story gets its own store (the `store` prop),
 * so no story sees another's toasts; an app uses `toast()` and a bare `<Toaster />`.
 */
function Raise({ store }: { store: ToastStore }) {
  return (
    <div className="flex flex-wrap gap-2">
      <Button onClick={() => store.toast({ title: "Scenario loaded", description: "eye_in_hand_ur5e · 12 captures" })}>
        Info
      </Button>
      <Button onClick={() => store.toast({ title: "Bake finished", tone: "success", duration: 400 })}>Success</Button>
      <Button onClick={() => store.toast({ title: "Pose unreachable", tone: "warn", description: "Capture 7 is outside the workspace." })}>
        Warn
      </Button>
      <Button
        onClick={() =>
          store.toast({ title: "Could not read scene.json", tone: "error", description: "Line 4: expected a number." })
        }
      >
        Error
      </Button>
    </div>
  );
}

/** The raise buttons and a `Toaster` over one story-local store. */
function Demo(args: ToasterProps) {
  const [store] = useState(createToastStore);
  return (
    <>
      <Raise store={store} />
      <Toaster {...args} store={store} />
    </>
  );
}

const meta = {
  title: "workbench/Toaster",
  component: Toaster,
  parameters: {
    docs: {
      description: {
        component: `The stack that shows notifications raised with \`toast({ title, description?, tone?, duration? })\` from
anywhere. Mount \`<Toaster />\` once near the root.

**Use** it for news that needs no answer: a file loaded, a bake finished, a background failure. Pass \`id\`
to \`toast()\` to update one in place (progress → result); \`toast.dismiss(id)\` removes it early.

**Don't** use it for anything that must be acknowledged (that is a \`Dialog\`) or for an error tied to one
control (that is the control's \`Field\` error). A toast has no actions besides dismissing it.

**Accessibility**: a "Notifications" landmark holding a polite live region; an \`error\` toast is a
\`role="alert"\`. Toasts dismiss themselves after \`duration\` (5 s, errors 8 s), paused while the pointer is
over the stack or focus is inside it, so a reader is never cut off. Each has a "Dismiss" button.`,
      },
    },
  },
  args: { limit: 5 },
  render: (args) => <Demo {...args} />,
} satisfies Meta<typeof Toaster>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Tones: Story = {
  play: async ({ canvas }) => {
    const region = within(document.body).getByRole("region", { name: "Notifications" });
    const stack = within(region);
    await userEvent.click(canvas.getByRole("button", { name: "Info" }));
    await expect(await stack.findByText("Scenario loaded")).toBeInTheDocument();
    await expect(stack.getByText("eye_in_hand_ur5e · 12 captures")).toBeInTheDocument();

    await userEvent.click(canvas.getByRole("button", { name: "Error" }));
    const alert = await stack.findByRole("alert");
    await expect(alert).toHaveTextContent("Could not read scene.json");
    await expect(alert).toHaveAttribute("data-tone", "error");

    await userEvent.click(canvas.getByRole("button", { name: "Warn" }));
    await expect(await stack.findByText("Pose unreachable")).toBeInTheDocument();
    await expect(region.querySelectorAll("li")).toHaveLength(3);

    await userEvent.click(within(alert).getByRole("button", { name: "Dismiss" }));
    await expect(stack.queryByRole("alert")).toBeNull();
    await expect(region.querySelectorAll("li")).toHaveLength(2);
  },
};

export const AutoDismiss: Story = {
  play: async ({ canvas }) => {
    const stack = within(within(document.body).getByRole("region", { name: "Notifications" }));
    // Clicked from the keyboard, so the pointer is not resting on the stack.
    canvas.getByRole("button", { name: "Success" }).focus();
    await userEvent.keyboard("{Enter}");
    await expect(await stack.findByText("Bake finished")).toBeInTheDocument();
    await waitFor(() => expect(stack.queryByText("Bake finished")).toBeNull(), { timeout: 2000 });
  },
};

export const PausedWhileHovered: Story = {
  play: async ({ canvas }) => {
    const region = within(document.body).getByRole("region", { name: "Notifications" });
    const stack = within(region);
    canvas.getByRole("button", { name: "Success" }).focus();
    await userEvent.keyboard("{Enter}");
    const toast = await stack.findByText("Bake finished");
    await userEvent.hover(toast);
    await new Promise((resolve) => setTimeout(resolve, 700));
    await expect(stack.getByText("Bake finished")).toBeInTheDocument();
    await userEvent.unhover(toast);
    await waitFor(() => expect(stack.queryByText("Bake finished")).toBeNull(), { timeout: 2000 });
  },
};

export const Limit: Story = {
  args: { limit: 2 },
  play: async ({ canvas }) => {
    const region = within(document.body).getByRole("region", { name: "Notifications" });
    await userEvent.click(canvas.getByRole("button", { name: "Info" }));
    await userEvent.click(canvas.getByRole("button", { name: "Warn" }));
    await userEvent.click(canvas.getByRole("button", { name: "Error" }));
    // The newest two are shown; the oldest waits.
    await waitFor(() => expect(region.querySelectorAll("li")).toHaveLength(2));
    await expect(within(region).queryByText("Scenario loaded")).toBeNull();
  },
};

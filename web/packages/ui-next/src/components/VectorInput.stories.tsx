import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fireEvent, fn, userEvent, waitFor } from "storybook/test";

import { VectorInput, type VectorInputProps } from "./VectorInput";

/**
 * Edit a field as a person does — focus it, change its text — through plain DOM events.
 * Not `userEvent`, and not a real `focus()`: user-event takes over any number field that gains
 * focus once it is set up, and rewrites every later programmatic value to its shortest numeric
 * form (`1.500` → `1.5`) — which would hide exactly the formatting under test.
 */
async function edit(field: HTMLElement, text: string) {
  await fireEvent.focusIn(field);
  await fireEvent.change(field, { target: { value: text } });
}

/** Change the text of a field that is already being edited (see `edit`). */
async function retype(field: HTMLElement, text: string) {
  await fireEvent.change(field, { target: { value: text } });
}

/** Leave a field (see `edit`). */
async function leave(field: HTMLElement) {
  await fireEvent.focusOut(field);
}

/** Controlled: the story keeps the vector and reports each change. */
function Stateful(args: VectorInputProps) {
  const [value, setValue] = useState<readonly number[]>(args.value);
  return (
    <div style={{ width: 360 }}>
      <VectorInput
        {...args}
        value={value}
        onValueChange={(next) => {
          setValue(next);
          args.onValueChange?.(next);
        }}
      />
    </div>
  );
}

const meta = {
  title: "ui-next/VectorInput",
  component: VectorInput,
  parameters: {
    docs: {
      description: {
        component: `A small vector edited as one row: an axis label before each number field, the unit once at the end.

**Use** it for a position, a set of angles, a per-axis scale — any quantity read as one thing with 2–4
components. \`precision\` sets the decimals shown at rest; while a field has focus it shows what is typed.
\`readOnly\` renders a compact mono readout.

**Don't** use it for unrelated numbers that happen to sit together (those are separate \`Field\`s), or for
long vectors (a table).

**Accessibility**: a \`role="group"\` named by \`aria-label\`; each field is named by its axis label and
described by the unit. Escape restores the value a field had on focus; Enter shows the formatted value.`,
      },
    },
  },
  args: { value: [0.1, -0.25, 1.2], unit: "m", precision: 3, step: 0.001, "aria-label": "Position", onValueChange: fn() },
  render: (args) => <Stateful {...args} />,
} satisfies Meta<typeof VectorInput>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvas, args }) => {
    await expect(canvas.getByRole("group", { name: "Position" })).toBeInTheDocument();
    const y = canvas.getByRole("spinbutton", { name: "y" });
    await expect(y).toHaveValue(-0.25);
    await expect(y).toHaveDisplayValue("-0.250");
    await expect(y).toHaveAccessibleDescription("m");

    await edit(y, "1.5");
    await expect(args.onValueChange).toHaveBeenLastCalledWith([0.1, 1.5, 1.2]);
    // While focused the field shows what was typed, not the formatted value…
    await expect(y).toHaveDisplayValue("1.5");
    // …and partial text is kept without being reported.
    await retype(y, "");
    await expect(args.onValueChange).toHaveBeenCalledTimes(1);
    await retype(y, "1.5");
    await leave(y);
    await waitFor(() => expect(y).toHaveDisplayValue("1.500"));
  },
};

export const Typing: Story = {
  play: async ({ canvas, args }) => {
    const z = canvas.getByRole("spinbutton", { name: "z" });
    await userEvent.clear(z);
    await userEvent.type(z, "-3.25");
    await expect(args.onValueChange).toHaveBeenLastCalledWith([0.1, -0.25, -3.25]);
  },
};

export const EnterAndEscape: Story = {
  play: async ({ canvas, args }) => {
    const x = canvas.getByRole("spinbutton", { name: "x" });
    await edit(x, "2");
    await fireEvent.keyDown(x, { key: "Enter" });
    await waitFor(() => expect(x).toHaveDisplayValue("2.000"));
    await expect(args.onValueChange).toHaveBeenLastCalledWith([2, -0.25, 1.2]);
    // Still focused after Enter: the value on focus is still 0.1.
    await retype(x, "7");
    await fireEvent.keyDown(x, { key: "Escape" });
    // Back to the value it had on focus.
    await expect(args.onValueChange).toHaveBeenLastCalledWith([0.1, -0.25, 1.2]);
    await waitFor(() => expect(x).toHaveDisplayValue("0.100"));
    // Keys while not editing do nothing.
    await leave(x);
    await fireEvent.keyDown(x, { key: "Escape" });
    await fireEvent.keyDown(x, { key: "Enter" });
    await expect(x).toHaveDisplayValue("0.100");
    // 2, then 7, then back to 0.1 — nothing since.
    await expect(args.onValueChange).toHaveBeenCalledTimes(3);
  },
};

export const ReadOnly: Story = {
  args: { readOnly: true },
  play: async ({ canvas }) => {
    const group = canvas.getByRole("group", { name: "Position" });
    await expect(group).toHaveAttribute("data-readonly");
    await expect(group).toHaveTextContent("x0.100 m·y-0.250 m·z1.200 m");
    await expect(canvas.queryByRole("spinbutton")).toBeNull();
  },
};

export const ReadOnlyUnitless: Story = {
  args: { readOnly: true, unit: undefined, precision: 1 },
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("group")).toHaveTextContent("x0.1·y-0.3·z1.2");
  },
};

export const Quaternion: Story = {
  args: { value: [0, 0, 0.7071, 0.7071], labels: ["qx", "qy", "qz", "qw"], unit: undefined, precision: 4, "aria-label": "Rotation" },
  play: async ({ canvas }) => {
    await expect(canvas.getAllByRole("spinbutton")).toHaveLength(4);
    await expect(canvas.getByRole("spinbutton", { name: "qw" })).toHaveDisplayValue("0.7071");
  },
};

export const Disabled: Story = {
  args: { disabled: true },
  play: async ({ canvas }) => {
    for (const field of canvas.getAllByRole("spinbutton")) await expect(field).toBeDisabled();
    await expect(canvas.getByRole("group")).toHaveAttribute("data-disabled");
  },
};

export const FallbackLabels: Story = {
  args: { value: [1, 2, 3, 4, 5], unit: undefined, precision: 0 },
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("spinbutton", { name: "w" })).toHaveDisplayValue("4");
    await expect(canvas.getByRole("spinbutton", { name: "4" })).toHaveDisplayValue("5");
  },
};

import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fireEvent, fn, userEvent, waitFor } from "storybook/test";

import { DensityProvider } from "@vitavision/ui";

import { PoseInput, type PoseInputProps, type PoseValue, type Quaternion, type RotationView, type Vec3 } from "./PoseInput";

/*
 * A test-only rotation view: roll/pitch/yaw as R = Rz(yaw) · Ry(pitch) · Rx(roll). The real one
 * belongs to the package that owns the frame conventions (@vitavision/three); this is here only
 * so the stories have something to call.
 */
function fromEuler([roll, pitch, yaw]: Vec3): Quaternion {
  const [cr, sr] = [Math.cos(roll / 2), Math.sin(roll / 2)];
  const [cp, sp] = [Math.cos(pitch / 2), Math.sin(pitch / 2)];
  const [cy, sy] = [Math.cos(yaw / 2), Math.sin(yaw / 2)];
  return [
    sr * cp * cy - cr * sp * sy,
    cr * sp * cy + sr * cp * sy,
    cr * cp * sy - sr * sp * cy,
    cr * cp * cy + sr * sp * sy,
  ];
}

function toEuler([x, y, z, w]: Quaternion): Vec3 {
  const sinPitch = Math.max(-1, Math.min(1, 2 * (w * y - z * x)));
  return [
    Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y)),
    Math.asin(sinPitch),
    Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)),
  ];
}

const RPY: RotationView = { toEuler, fromEuler };

/** A camera 0.1 m ahead, 0.2 m left and 0.3 m up, yawed 90°. */
const POSE: PoseValue = { rotation: fromEuler([0, 0, Math.PI / 2]), translation: [0.1, 0.2, 0.3] };

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

/** Leave a field (see `edit`). */
async function leave(field: HTMLElement) {
  await fireEvent.focusOut(field);
}

/** Controlled: the story keeps the pose and reports each change. */
function Stateful(args: PoseInputProps) {
  const [value, setValue] = useState(args.value);
  return (
    <div style={{ width: 420 }}>
      <PoseInput
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
  title: "ui-next/PoseInput",
  component: PoseInput,
  parameters: {
    docs: {
      description: {
        component: `An SE(3) pose in the wire form \`{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }\`, shown and
edited as a translation (m, or mm with \`translationUnit="mm"\`) and three angles in degrees.

**Use** it wherever a pose is inspected or typed: a camera's mount, a target's placement. Pass the
\`rotationView\` from the package that owns the frame conventions — this component does no rotation
mathematics, so "roll, pitch, yaw" means whatever that view says. The value is always metres.

**Don't** use it for a pose that is only ever dragged in a viewport (show it \`readOnly\`), or to edit a
quaternion's raw components (that is a \`VectorInput\`).

**Accessibility**: a \`role="group"\` holding a "Translation" and a "Rotation" group of named fields (x, y, z;
roll, pitch, yaw), each described by its unit. \`readOnly\` renders a one-line mono readout.`,
      },
    },
  },
  args: { value: POSE, rotationView: RPY, onValueChange: fn() },
  render: (args) => <Stateful {...args} />,
} satisfies Meta<typeof PoseInput>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvas, args }) => {
    await expect(canvas.getByRole("group", { name: "Pose" })).toBeInTheDocument();
    const x = canvas.getByRole("spinbutton", { name: "x" });
    await expect(x).toHaveDisplayValue("0.1000");
    await expect(x).toHaveAccessibleDescription("m");
    const yaw = canvas.getByRole("spinbutton", { name: "yaw" });
    await expect(yaw).toHaveDisplayValue("90.00");
    await expect(yaw).toHaveAccessibleDescription("°");

    await userEvent.clear(x);
    await userEvent.type(x, "0.5");
    await expect(args.onValueChange).toHaveBeenLastCalledWith({ rotation: POSE.rotation, translation: [0.5, 0.2, 0.3] });

    // A yaw past 180° is kept as typed, not shown back as −170°.
    await edit(yaw, "190");
    await leave(yaw);
    await waitFor(() => expect(yaw).toHaveDisplayValue("190.00"));
    const last = lastPose(args.onValueChange);
    const expected = fromEuler([0, 0, (190 * Math.PI) / 180]);
    for (const [index, component] of last.rotation.entries()) {
      await expect(component).toBeCloseTo(expected[index] ?? Number.NaN, 12);
    }
    await expect(last.translation).toEqual([0.5, 0.2, 0.3]);
  },
};

/** The last pose passed to the `onValueChange` spy. */
function lastPose(spy: PoseInputProps["onValueChange"]): PoseValue {
  const calls = (spy as unknown as { mock: { calls: [PoseValue][] } }).mock.calls;
  const last = calls.at(-1)?.[0];
  if (!last) throw new Error("onValueChange was not called");
  return last;
}

export const Millimetres: Story = {
  args: { translationUnit: "mm" },
  play: async ({ canvas, args }) => {
    const z = canvas.getByRole("spinbutton", { name: "z" });
    await expect(z).toHaveDisplayValue("300.0");
    await expect(z).toHaveAccessibleDescription("mm");
    await userEvent.clear(z);
    await userEvent.type(z, "250");
    const last = lastPose(args.onValueChange);
    await expect(last.translation[2]).toBeCloseTo(0.25, 12);
  },
};

export const ExternalChange: Story = {
  render: (args) => {
    function Harness() {
      const [value, setValue] = useState(args.value);
      return (
        <div style={{ width: 420 }} className="flex flex-col gap-2">
          <PoseInput {...args} value={value} onValueChange={setValue} />
          <button type="button" onClick={() => setValue({ ...value, rotation: fromEuler([0, Math.PI / 6, 0]) })}>
            Tilt from the viewport
          </button>
        </div>
      );
    }
    return <Harness />;
  },
  play: async ({ canvas }) => {
    const yaw = canvas.getByRole("spinbutton", { name: "yaw" });
    await edit(yaw, "45");
    await leave(yaw);
    await waitFor(() => expect(yaw).toHaveDisplayValue("45.00"));
    // A rotation set elsewhere replaces the typed angles.
    await userEvent.click(canvas.getByRole("button", { name: "Tilt from the viewport" }));
    await waitFor(() => expect(canvas.getByRole("spinbutton", { name: "pitch" })).toHaveDisplayValue("30.00"));
    await expect(yaw).toHaveDisplayValue("0.00");
  },
};

export const ReadOnly: Story = {
  args: { readOnly: true },
  play: async ({ canvas }) => {
    const group = canvas.getByRole("group", { name: "Pose" });
    await expect(group).toHaveAttribute("data-readonly");
    await expect(group).toHaveTextContent("t0.1000 0.2000 0.3000 m·rpy0.00 0.00 90.00 °");
    await expect(canvas.queryByRole("spinbutton")).toBeNull();
  },
};

export const CustomAngles: Story = {
  args: {
    rotationView: { ...RPY, labels: ["rx", "ry", "rz"] },
    translationPrecision: 2,
    rotationPrecision: 0,
    "aria-label": "Camera mount",
  },
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("group", { name: "Camera mount" })).toBeInTheDocument();
    await expect(canvas.getByRole("spinbutton", { name: "rz" })).toHaveDisplayValue("90");
    await expect(canvas.getByRole("spinbutton", { name: "x" })).toHaveDisplayValue("0.10");
  },
};

export const Disabled: Story = {
  args: { disabled: true },
  play: async ({ canvas }) => {
    await expect(canvas.getAllByRole("spinbutton")).toHaveLength(6);
    for (const field of canvas.getAllByRole("spinbutton")) await expect(field).toBeDisabled();
  },
};

export const Compact: Story = {
  render: (args) => (
    <DensityProvider value="compact">
      <Stateful {...args} />
    </DensityProvider>
  ),
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("spinbutton", { name: "roll" }).className).toContain("h-7");
  },
};

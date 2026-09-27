/*
 * A number, with its unit written in the field.
 *
 * A focal length, a focus distance, a translation: the quantities an instrument panel edits
 * all have units, and a unit stated once in the label is a unit the reader has to carry in
 * their head down a column of fields. So the unit sits inside the field, after the number —
 * quiet, in mono, and not part of the value.
 *
 * This is `@vitavision/ui`'s `NumberInput` plus `unit`, composed rather than copied: without
 * a unit it renders ui's component untouched. The upstream PR adds the prop to ui's own
 * component with this same behaviour, and this one is then deleted.
 */

import { useId } from "react";
import type { ComponentProps } from "react";

import { NumberInput as UiNumberInput } from "@vitavision/ui";

/** Props of `NumberInput`: everything `@vitavision/ui`'s `NumberInput` takes, plus `unit`. */
export type NumberInputProps = ComponentProps<typeof UiNumberInput> & {
  /**
   * The quantity's unit (`mm`, `m`, `°`, `px`), shown after the number inside the field.
   * It is not part of the value, and it is announced as the field's description
   * (`aria-describedby`): after the caller's own `aria-describedby`, before a surrounding
   * `Field`'s description.
   */
  unit?: string | undefined;
};

/**
 * A number input — `@vitavision/ui`'s `NumberInput` (mono, `type="number"`, a `Field`'s
 * description and error wired in) — with an optional `unit` written inside the field after
 * the value.
 *
 * Without `unit` it is exactly ui's `NumberInput`. With one, the input sits in a
 * `relative` full-width wrapper (`data-unit` carries the unit), keeps room on its right for
 * the unit, and is described by it. `className` and every other prop, `ref` included, go to
 * the `<input>`; size a field with a unit through its container.
 */
export function NumberInput({ unit, style, "aria-describedby": describedBy, ...rest }: NumberInputProps) {
  const unitId = useId();
  if (unit === undefined || unit === "") {
    return <UiNumberInput {...rest} style={style} aria-describedby={describedBy} />;
  }

  return (
    <span data-unit={unit} className="relative flex w-full min-w-0 items-center">
      <UiNumberInput
        {...rest}
        aria-describedby={describedBy === undefined ? unitId : `${describedBy} ${unitId}`}
        // Room for the unit: its width in the unit's own characters, plus the field's padding.
        style={{ paddingInlineEnd: `calc(${unit.length}ch + 1rem)`, ...style }}
      />
      {/* `aria-hidden` keeps the unit out of the field's *name* when a `<label>` wraps it;
          `aria-describedby` still reads it, as the description. */}
      <span
        id={unitId}
        aria-hidden
        className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center font-mono text-xs text-fg-subtle select-none"
      >
        {unit}
      </span>
    </span>
  );
}

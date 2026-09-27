/*
 * A small vector — a position, a set of angles, a scale — edited as one row.
 *
 * Three separate `Field`s for x, y and z spend three labels and three rows on what a reader
 * thinks of as one quantity. This is one row: a quiet axis letter before each number, the
 * unit once at the end, and one name for the whole group.
 *
 * Each field keeps the text being typed while it has focus and shows the formatted value
 * otherwise (see `numberText.ts`), so `precision` never fights the cursor. Every valid
 * keystroke is reported, so whatever the vector drives follows the typing.
 */

import { useId, useState } from "react";
import type { KeyboardEvent } from "react";

import { ReadoutStrip, cn } from "@vitavision/ui";

import { NumberInput } from "./NumberInput";
import { formatNumber, parseNumber } from "./numberText";

const DEFAULT_LABELS = ["x", "y", "z", "w"];

/** Props of `VectorInput`. */
export interface VectorInputProps {
  /** The components. Its length is the number of fields. */
  value: readonly number[];
  /** Called with the whole vector (a new array) whenever one component changes to a valid number. */
  onValueChange?: ((value: number[]) => void) | undefined;
  /** Each component's label, shown before its field and naming it. Defaults to x, y, z, w. */
  labels?: readonly string[] | undefined;
  /** The unit of every component, shown once after the row (`m`, `mm`, `°`). */
  unit?: string | undefined;
  /** The arrow-key step. Defaults to `any` (the browser then steps by 1). */
  step?: number | undefined;
  /** Decimals shown when a field is not being edited. Defaults to 3. */
  precision?: number | undefined;
  /** The smallest accepted component. */
  min?: number | undefined;
  /** The largest accepted component. */
  max?: number | undefined;
  /** Show the values as a compact mono readout instead of fields. */
  readOnly?: boolean | undefined;
  /** Blocks every field. */
  disabled?: boolean | undefined;
  /** Names the group (`role="group"`). */
  "aria-label": string;
  /** Merged with the row's own classes through `cn`. */
  className?: string | undefined;
}

/**
 * A row of number fields editing one small vector, with the axis labels before the fields
 * and the unit once after them.
 *
 * Controlled. A `role="group"` named by `aria-label`, each field named by its label and
 * described by the unit. While a field has focus it shows what is typed; on blur or Enter it
 * shows the value at `precision`; Escape restores the value it had on focus. With `readOnly`
 * it renders a `ReadoutStrip` (`x 0.100 m · y …`) instead. The row carries `data-readonly`
 * and `data-disabled`.
 */
export function VectorInput({
  value,
  onValueChange,
  labels = DEFAULT_LABELS,
  unit,
  step,
  precision = 3,
  min,
  max,
  readOnly = false,
  disabled = false,
  "aria-label": ariaLabel,
  className,
}: VectorInputProps) {
  const unitId = useId();
  const labelOf = (index: number) => labels[index] ?? String(index);

  if (readOnly) {
    return (
      <div role="group" aria-label={ariaLabel} data-readonly="" className={cn("min-w-0", className)}>
        <ReadoutStrip
          items={value.map((component, index) => ({
            label: labelOf(index),
            value: unit ? `${formatNumber(component, precision)} ${unit}` : formatNumber(component, precision),
          }))}
        />
      </div>
    );
  }

  const change = (index: number, component: number) => {
    if (component === value[index]) return;
    onValueChange?.(value.map((current, i) => (i === index ? component : current)));
  };

  return (
    <div
      role="group"
      aria-label={ariaLabel}
      data-disabled={disabled ? "" : undefined}
      className={cn("flex min-w-0 items-center gap-1.5", className)}
    >
      {value.map((component, index) => (
        // The components are positional: index is their identity.
        // eslint-disable-next-line @eslint-react/no-array-index-key -- see above
        <label key={index} className="flex min-w-0 flex-1 items-center gap-1">
          <span className="shrink-0 font-mono text-[11px] text-fg-subtle">{labelOf(index)}</span>
          <ComponentField
            value={component}
            precision={precision}
            step={step}
            min={min}
            max={max}
            disabled={disabled}
            describedBy={unit ? unitId : undefined}
            onCommit={(next) => change(index, next)}
          />
        </label>
      ))}
      {unit && (
        <span id={unitId} className="shrink-0 font-mono text-xs text-fg-subtle">
          {unit}
        </span>
      )}
    </div>
  );
}

/** One component: formatted at rest, the typed text while focused. */
function ComponentField({
  value,
  precision,
  step,
  min,
  max,
  disabled,
  describedBy,
  onCommit,
}: {
  value: number;
  precision: number;
  step: number | undefined;
  min: number | undefined;
  max: number | undefined;
  disabled: boolean;
  describedBy: string | undefined;
  onCommit: (value: number) => void;
}) {
  // Non-null while the field is being edited: the text as typed, and the value on focus.
  const [draft, setDraft] = useState<{ text: string; initial: number } | null>(null);
  const formatted = formatNumber(value, precision);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      setDraft((current) => current && { ...current, text: formatted });
    } else if (event.key === "Escape" && draft) {
      event.preventDefault();
      onCommit(draft.initial);
      setDraft({ ...draft, text: formatNumber(draft.initial, precision) });
    }
  };

  return (
    <NumberInput
      value={draft?.text ?? formatted}
      step={step ?? "any"}
      min={min}
      max={max}
      disabled={disabled}
      aria-describedby={describedBy}
      onFocus={() => setDraft({ text: formatted, initial: value })}
      onBlur={() => setDraft(null)}
      onKeyDown={onKeyDown}
      onChange={(event) => {
        const text = event.currentTarget.value;
        setDraft((current) => ({ text, initial: current?.initial ?? value }));
        const parsed = parseNumber(text);
        if (parsed !== null) onCommit(parsed);
      }}
      className="min-w-0 px-1.5"
    />
  );
}

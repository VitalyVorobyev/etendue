/*
 * Numbers as a field shows them and as a person types them.
 *
 * A field bound straight to a number re-formats under the cursor: typing `0.` into a field
 * showing `0.000` becomes `0` before the next digit arrives. So the multi-number fields keep
 * the typed text while they have focus and show the formatted value otherwise; these are the
 * two conversions between the two.
 */

/**
 * A number at a fixed number of decimals, without a negative zero (`-0.000` reads as a sign
 * error in a coordinate readout).
 *
 * @param value - The number. Non-finite values print as `NaN`, `Infinity`, `-Infinity`.
 * @param precision - Decimals, 0–20.
 * @returns The text.
 */
export function formatNumber(value: number, precision: number): string {
  if (!Number.isFinite(value)) return String(value);
  const text = value.toFixed(precision);
  return /^-0(?:\.0*)?$/.test(text) ? text.slice(1) : text;
}

/**
 * The number a field's text means, if it means one.
 *
 * @param text - What is in the field.
 * @returns The number, or `null` for empty, partial (`-`, `1e`) or non-numeric text.
 */
export function parseNumber(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

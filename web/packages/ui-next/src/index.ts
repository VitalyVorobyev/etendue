/**
 * `@vitavision/ui-next` — additions to `@vitavision/ui` incubating in etendue: `NumberInput`
 * with a `unit`, `VectorInput`, `PoseInput`. Each moves into `@vitavision/ui` by PR, and this
 * package then disappears.
 *
 * Import `@vitavision/ui-next/styles.css` after `@vitavision/ui/styles.css`.
 *
 * @packageDocumentation
 */

export { NumberInput, type NumberInputProps } from "./components/NumberInput";
export { VectorInput, type VectorInputProps } from "./components/VectorInput";
export {
  PoseInput,
  type PoseInputProps,
  type PoseValue,
  type Quaternion,
  type RotationView,
  type Vec3,
} from "./components/PoseInput";
export { formatNumber, parseNumber } from "./components/numberText";

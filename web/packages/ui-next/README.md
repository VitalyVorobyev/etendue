# @vitavision/ui-next

Additions to [`@vitavision/ui`](https://github.com/VitalyVorobyev/lab-ui/tree/main/packages/ui)
incubating in etendue (`web/packages/ui-next`). Each moves into `@vitavision/ui` by PR, with its
stories and tests; this package then disappears. Same tokens, same density, same conventions.

```bash
bun add @vitavision/ui-next @vitavision/ui
```

```css
@import "tailwindcss";
@import "@vitavision/ui/styles.css";
@import "@vitavision/ui-next/styles.css";
```

| Export | What | Upstream |
|---|---|---|
| `NumberInput` | ui's `NumberInput` plus `unit`: the unit written inside the field after the value, mono and muted, not part of the value, announced as the field's description. Without `unit` it renders ui's component unchanged. | a `unit` prop on ui's `NumberInput` |
| `VectorInput` | A row of number fields for one small vector: axis labels (`x y z` by default), the unit once at the end, `precision` at rest and the typed text while editing; `readOnly` renders a `ReadoutStrip`. | new component |
| `PoseInput` | An SE(3) pose in the wire form `{ rotation: [qx, qy, qz, qw], translation: [tx, ty, tz] }`, edited as a translation (m or mm) and roll/pitch/yaw in degrees; `readOnly` renders a one-line readout. | new component |
| `formatNumber`, `parseNumber` | The text ↔ number rules the fields share. | alongside `VectorInput` |

`PoseInput` does no rotation mathematics. The quaternion ↔ angles conversion — and so what
"roll, pitch, yaw" means — is passed in as a `RotationView` by the package that owns the frame
conventions:

```tsx
<PoseInput
  value={camera.world_se3_self}
  onValueChange={setPose}
  rotationView={{ toEuler: quatToRpy, fromEuler: rpyToQuat }}
  translationUnit="mm"
/>
```

## License

Licensed under either of

- Apache License, Version 2.0
- MIT license

at your option.

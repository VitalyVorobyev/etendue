# @etendue/wasm

[etendue](https://github.com/VitalyVorobyev/etendue)'s kernel in WebAssembly: load a scene and its
robots, bake scenarios with the robots' kinematics, project points through calibration-rs camera
models, back-project pixels, and build remap tables. The math is the Rust crates'; the package
only marshals, and the TypeScript types are generated from etendue's JSON Schemas.

```ts
import init, { EtendueScene } from "@etendue/wasm";

await init();
const scene = new EtendueScene(sceneSpec, [{ id: "ur5e", manifest, urdf }]);
const baked = scene.bake(scenarioSpec);
```

The host reads the files (the package does no I/O). Documentation:
<https://vitalyvorobyev.github.io/etendue>.

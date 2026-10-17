# etendue-cli

The `etendue` command line of [etendue](https://github.com/VitalyVorobyev/etendue), and the
`etendue_cli` library behind it.

```text
etendue validate <scene> [scenario]                  check a cell and its scenario
etendue bake     <scene> <scenario> -o <baked.json>  every frame's pose at every sample
etendue gt       <scene> <scenario> -o <dir>         ground truth as a calibration-rs dataset
etendue render   <scene> <scenario> -o <dir>         Blender renders on the real camera models
etendue detect   <dir>                               board detection, checked against the truth
etendue scenario from-poses <poses> --robot <id> -o <scenario.json>
```

Rendering needs Blender 5.1.1. Documentation: <https://vitalyvorobyev.github.io/etendue>.

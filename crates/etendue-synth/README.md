# etendue-synth

Synthetic calibration data for [etendue](https://github.com/VitalyVorobyev/etendue).

- **Ground truth**: every board corner and laser-stripe point projected into every camera at
  every capture, with visibility, and emitted as a calibration-rs `DatasetSpec` plus `gt.json`.
- **Boards** as calib-targets prints them (chessboard, ChArUco).
- **Rendering support**: the canonical pinhole a renderer draws for each camera and the remap
  table onto the real camera model (distortion, Scheimpflug tilt), render jobs, and a sensor model
  (exposure, noise, quantisation). EXR and PNG I/O are behind the `images` feature.

Documentation: <https://vitalyvorobyev.github.io/etendue>.

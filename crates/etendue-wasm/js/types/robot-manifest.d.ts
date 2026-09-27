/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * A robot asset manifest (`robot.json`), version 1.
 *
 * Written by `tools/robot-assets/build.py`; consumed by
 * `etendue-kinematics` (joint limits, base and TCP links) and by the web
 * viewer (per-link meshes).
 */
export interface RobotManifest {
  /**
   * The robot's base link (the frame a [`RobotSpec`] places).
   */
  base_link: string;
  /**
   * Robot model id (e.g. `"ur5e"`).
   */
  id: string;
  /**
   * Movable joints on the path `base_link → tcp_link`, in that order. This
   * order defines the joint vector `q` everywhere.
   */
  joints: ManifestJoint[];
  license: ManifestLicense;
  /**
   * Human-readable name.
   */
  name: string;
  source: ManifestSource;
  /**
   * The link whose pose the controller reports (the calibration-rs
   * "gripper" frame; `robot_poses` are exported as `base_se3_<tcp_link>`).
   */
  tcp_link: string;
  /**
   * Expanded URDF, path relative to this manifest. Used for kinematics.
   */
  urdf: string;
  /**
   * Manifest format version; must be `1`.
   */
  version: number;
  /**
   * Per-link visual meshes (links without visual geometry are absent).
   */
  visuals: ManifestVisual[];
}
/**
 * Motion limits of one joint.
 */
export interface ManifestJoint {
  /**
   * Provenance of the limits (file and key, datasheet, or documented
   * default).
   */
  limit_source: string;
  /**
   * Lower position limit (rad or m).
   */
  lower: number;
  /**
   * Maximum acceleration (rad/s² or m/s²), `> 0`.
   */
  max_acceleration: number;
  /**
   * Maximum speed (rad/s or m/s), `> 0`.
   */
  max_velocity: number;
  /**
   * URDF joint name.
   */
  name: string;
  /**
   * Upper position limit (rad or m).
   */
  upper: number;
}
/**
 * Licences of the description and meshes.
 */
export interface ManifestLicense {
  /**
   * Licence of the meshes (SPDX id or `LicenseRef-…` with a description).
   */
  meshes: string;
  /**
   * Whether the meshes may be redistributed (committed / published).
   */
  meshes_redistributable: boolean;
  /**
   * Evidence and remarks.
   */
  notes: string;
  /**
   * Licence of the URDF / xacro (SPDX id).
   */
  urdf: string;
}
/**
 * Where the description came from.
 */
export interface ManifestSource {
  /**
   * xacro / URDF entry file inside the repository.
   */
  entry: string;
  /**
   * Repository URL.
   */
  repository: string;
  /**
   * Pinned git commit SHA.
   */
  revision: string;
  /**
   * xacro arguments used for expansion.
   */
  xacro_args: {
    [k: string]: string;
  };
}
/**
 * A link's visual mesh.
 */
export interface ManifestVisual {
  /**
   * URDF link name.
   */
  link: string;
  /**
   * glTF binary in the **link frame** (URDF visual origin and scale
   * applied), path relative to the manifest.
   */
  mesh: string;
}

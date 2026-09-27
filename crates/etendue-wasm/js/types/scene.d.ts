/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * A scene: every frame, robot, and entity, version 1.
 *
 * All ids — of frames, robots, rigs, cameras, lasers, lights, targets, and
 * parts — share one namespace.
 */
export interface SceneSpec {
  /**
   * Cameras.
   */
  cameras?: CameraSpec[];
  /**
   * Free-text description.
   */
  description?: string | null;
  /**
   * Named auxiliary frames (fixtures, mounting plates, tool offsets).
   */
  frames?: FrameSpec[];
  /**
   * Line lasers.
   */
  lasers?: LaserSpec[];
  /**
   * Light sources.
   */
  lights?: LightSpec[];
  /**
   * Passive mesh parts.
   */
  parts?: PartSpec[];
  /**
   * Camera rigs.
   */
  rigs?: RigSpec[];
  /**
   * Robots.
   */
  robots?: RobotSpec[];
  /**
   * Targets (calibration boards, plain planes).
   */
  targets?: TargetSpec[];
  /**
   * Format version; must be [`SCENE_VERSION`].
   */
  version: number;
}
/**
 * A camera. Its projection model is a calibration-rs [`CameraParams`] —
 * the same type calibration-rs calibrates and exports.
 */
export interface CameraSpec {
  /**
   * Unique id.
   */
  id: string;
  params: CameraParams;
  /**
   * Parent frame (usually a rig).
   */
  parent: string;
  parent_se3_self: Iso3Schema;
  /**
   * Image size in pixels, `[width, height]`.
   *
   * @minItems 2
   * @maxItems 2
   */
  resolution: [number, number];
}
/**
 * Projection / distortion / sensor / intrinsics.
 */
export interface CameraParams {
  /**
   * Distortion model parameters.
   */
  distortion:
    | {
        type: "none";
      }
    | {
        /**
         * Maximum Newton iterations of [`DistortionModel::undistort`] (0 → 20).
         */
        iters: number;
        /**
         * Radial coefficient k1.
         */
        k1: number;
        /**
         * Radial coefficient k2.
         */
        k2: number;
        /**
         * Radial coefficient k3.
         */
        k3: number;
        /**
         * Tangential coefficient p1.
         */
        p1: number;
        /**
         * Tangential coefficient p2.
         */
        p2: number;
        type: "brown_conrady5";
      }
    | {
        /**
         * Maximum Newton iterations of [`DistortionModel::undistort`] (0 → 20).
         */
        iters: number;
        /**
         * Numerator radial coefficient k1.
         */
        k1: number;
        /**
         * Numerator radial coefficient k2.
         */
        k2: number;
        /**
         * Numerator radial coefficient k3.
         */
        k3: number;
        /**
         * Denominator radial coefficient k4.
         */
        k4: number;
        /**
         * Denominator radial coefficient k5.
         */
        k5: number;
        /**
         * Denominator radial coefficient k6.
         */
        k6: number;
        /**
         * Tangential coefficient p1.
         */
        p1: number;
        /**
         * Tangential coefficient p2.
         */
        p2: number;
        type: "rational";
      }
    | {
        /**
         * Maximum Newton iterations of [`DistortionModel::undistort`] (0 → 20).
         */
        iters: number;
        /**
         * Radial coefficient k1.
         */
        k1: number;
        /**
         * Radial coefficient k2.
         */
        k2: number;
        /**
         * Radial coefficient k3.
         */
        k3: number;
        /**
         * Tangential coefficient p1.
         */
        p1: number;
        /**
         * Tangential coefficient p2.
         */
        p2: number;
        /**
         * Thin-prism coefficient s1 (x correction, r²).
         */
        s1: number;
        /**
         * Thin-prism coefficient s2 (x correction, r⁴).
         */
        s2: number;
        /**
         * Thin-prism coefficient s3 (y correction, r²).
         */
        s3: number;
        /**
         * Thin-prism coefficient s4 (y correction, r⁴).
         */
        s4: number;
        type: "thin_prism";
      }
    | {
        /**
         * Division distortion coefficient.
         */
        lambda: number;
        type: "division";
      };
  /**
   * Intrinsics model parameters.
   */
  intrinsics: {
    /**
     * Principal point X coordinate in pixels.
     */
    cx: number;
    /**
     * Principal point Y coordinate in pixels.
     */
    cy: number;
    /**
     * Focal length in pixels along X.
     */
    fx: number;
    /**
     * Focal length in pixels along Y.
     */
    fy: number;
    /**
     * Skew term (typically 0).
     */
    skew: number;
    type: "fx_fy_cx_cy_skew";
  };
  /**
   * Projection model parameters.
   */
  projection: {
    type: "pinhole";
  };
  /**
   * Sensor model parameters.
   */
  sensor:
    | {
        type: "identity";
      }
    | {
        /**
         * Row-major homography matrix mapping normalized to sensor coordinates.
         *
         * @minItems 3
         * @maxItems 3
         */
        h: [[number, number, number], [number, number, number], [number, number, number]];
        type: "homography";
      }
    | {
        /**
         * Tilt around X axis in radians (alias: tau_x).
         */
        tilt_x: number;
        /**
         * Tilt around Y axis in radians (alias: tau_y).
         */
        tilt_y: number;
        type: "scheimpflug";
      };
}
/**
 * Pose of the camera in its parent (maps camera → parent).
 */
export interface Iso3Schema {
  /**
   * Unit quaternion `[qx, qy, qz, qw]` (i, j, k, w order).
   *
   * @minItems 4
   * @maxItems 4
   */
  rotation: [number, number, number, number];
  /**
   * Translation `[tx, ty, tz]` in meters.
   *
   * @minItems 3
   * @maxItems 3
   */
  translation: [number, number, number];
}
/**
 * A named auxiliary frame — a fixture, a mounting plate, a TCP offset.
 */
export interface FrameSpec {
  /**
   * Unique id (shared namespace with every other entity and robot).
   */
  id: string;
  /**
   * Parent frame.
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}
/**
 * A line laser: a flat fan with a Gaussian-beam cross-section (the
 * `etendue-core` `LaserEntity` parameters).
 */
export interface LaserSpec {
  /**
   * Beam-waist `1/e²` radius at the laser origin, in metres.
   */
  beam_waist_m: number;
  /**
   * Fan half-angle in radians, in `(0, π/2)`.
   */
  fan_half_angle: number;
  /**
   * Fan reach along its central ray, in metres.
   */
  fan_length: number;
  /**
   * Unique id.
   */
  id: string;
  /**
   * Parent frame.
   */
  parent: string;
  parent_se3_self: Iso3Schema;
  /**
   * Emission wavelength in nanometres.
   */
  wavelength_nm: number;
}
/**
 * A light source.
 */
export interface LightSpec {
  /**
   * Linear RGB colour, each channel in `[0, 1]`.
   *
   * @minItems 3
   * @maxItems 3
   */
  color: [number, number, number];
  /**
   * Unique id.
   */
  id: string;
  /**
   * Parent frame.
   */
  parent: string;
  parent_se3_self: Iso3Schema;
  /**
   * Radiant power in watts.
   */
  power_w: number;
  /**
   * Emitter shape.
   */
  shape:
    | {
        /**
         * Emitter radius in metres (`0` = ideal point).
         */
        radius_m: number;
        type: "point";
      }
    | {
        /**
         * Soft-edge fraction of the cone, in `[0, 1]`.
         */
        blend: number;
        /**
         * Full cone angle in radians, in `(0, π)`.
         */
        cone_angle: number;
        type: "spot";
      }
    | {
        /**
         * Extent `[x, y]` in metres.
         *
         * @minItems 2
         * @maxItems 2
         */
        size_m: [number, number];
        type: "area";
      };
}
/**
 * A passive rigid part rendered from a mesh (a fixture, a workpiece).
 */
export interface PartSpec {
  /**
   * Unique id.
   */
  id: string;
  /**
   * Mesh file (glTF binary), path relative to the scene file. Vertices
   * are in the part frame, metres.
   */
  mesh: string;
  /**
   * Parent frame.
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}
/**
 * A rigid mount that groups cameras (and optionally lasers). Cameras attach
 * to it with `parent_se3_self = rig_se3_cam`, the direction of calibration-rs
 * `DeviceSpec` `rig_se3_cam`.
 */
export interface RigSpec {
  /**
   * Unique id.
   */
  id: string;
  /**
   * Parent frame (e.g. `"world"` or `"<robot>/tool0"` for eye-in-hand).
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}
/**
 * A robot placed in a scene.
 *
 * The robot contributes one frame per URDF link, named
 * `"<id>/<link_name>"`. `parent_se3_self` places the manifest's
 * [`base_link`](RobotManifest::base_link) in the parent frame.
 */
export interface RobotSpec {
  /**
   * Unique id (shared namespace with every entity).
   */
  id: string;
  /**
   * Joint positions at `t = 0`, in manifest joint order (rad or m).
   * Defaults to all zeros.
   */
  initial_q?: number[] | null;
  /**
   * Path to the robot's `robot.json` manifest, relative to the scene file.
   */
  manifest: string;
  /**
   * Parent frame of the robot base.
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}
/**
 * A target surface.
 */
export interface TargetSpec {
  /**
   * What the target is.
   */
  geometry:
    | {
        /**
         * Board layout (`kind`-tagged: chessboard, charuco, puzzleboard,
         * ringgrid).
         */
        board:
          | {
              /**
               * Number of interior corners along the cols axis.
               */
              cols: number;
              kind: "chessboard";
              /**
               * Number of interior corners along the rows axis.
               */
              rows: number;
              /**
               * Edge length of one square in metres.
               */
              square_size_m: number;
            }
          | {
              /**
               * Number of squares along the cols axis.
               */
              cols: number;
              /**
               * ArUco dictionary identifier (e.g. `"DICT_4X4_50"`). The
               * detector validates this against the supported set.
               */
              dictionary: string;
              kind: "charuco";
              /**
               * Edge length of an embedded marker in metres.
               */
              marker_size_m: number;
              /**
               * Number of squares along the rows axis (full grid, not
               * interior corners).
               */
              rows: number;
              /**
               * Edge length of one square in metres.
               */
              square_size_m: number;
            }
          | {
              /**
               * Edge length of one cell in metres.
               */
              cell_size_m: number;
              kind: "puzzleboard";
              /**
               * Named layout (e.g. `"puzzle_130x130"`).
               */
              layout: string;
            }
          | {
              kind: "ringgrid";
              /**
               * Number of columns in the longest (even-indexed) row. Shorter
               * rows are derived by the hex-lattice layout.
               */
              long_row_cols: number;
              /**
               * Inner ring radius in metres.
               */
              marker_inner_radius_m: number;
              /**
               * Outer ring radius in metres.
               */
              marker_outer_radius_m: number;
              /**
               * Width of each ring band in metres.
               */
              marker_ring_width_m: number;
              /**
               * Center-to-center spacing between adjacent markers in metres.
               */
              pitch_m: number;
              /**
               * Number of marker rows.
               */
              rows: number;
            };
        type: "board";
      }
    | {
        /**
         * Extent along local Y, in metres.
         */
        height: number;
        type: "rectangle";
        /**
         * Extent along local X, in metres.
         */
        width: number;
      };
  /**
   * Unique id.
   */
  id: string;
  /**
   * Parent frame (`"world"`, a fixture, or `"<robot>/tool0"` for
   * eye-to-hand).
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}

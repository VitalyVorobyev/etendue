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
  /**
   * The camera's physical optics, if it has them: then its frame is its body's (ADR 0008).
   */
  optics?: OpticsSpec | null;
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
 * A camera's physical optics.
 */
export interface OpticsSpec {
  /**
   * Distance from the image-plane datum to the plane in focus, metres, as a lens's focus scale
   * reads it.
   */
  focus_distance_m: number;
  lens: LensOptics;
  sensor: SensorOptics;
  /**
   * The sensor's tilt `[τx, τy]` about the image-plane datum, radians, in calibration-rs's
   * (OpenCV's) convention.
   *
   * @minItems 2
   * @maxItems 2
   */
  tilt_rad?: [number, number];
}
/**
 * The lens.
 */
export interface LensOptics {
  /**
   * Signed distance from H to the entrance pupil, metres (positive toward the object).
   */
  entrance_pupil_m?: number;
  /**
   * f-number: the focal length over the entrance pupil diameter.
   */
  f_number: number;
  /**
   * Effective focal length, metres.
   */
  focal_length_m: number;
  /**
   * How the lens focuses.
   */
  focusing?: {
    type: "unit";
  };
  /**
   * What the lens is, e.g. `Kowa LM25HC`.
   */
  name?: string | null;
  /**
   * Signed distance from the rear principal point H′ to the front one H, metres (positive
   * when H lies on the object side).
   */
  principal_gap_m?: number;
}
/**
 * The sensor.
 */
export interface SensorOptics {
  /**
   * What the sensor is, e.g. `Sony IMX174`.
   */
  name?: string | null;
  /**
   * Pixel pitch, metres.
   */
  pixel_pitch_m: number;
}
/**
 * Projection / distortion / sensor / intrinsics. With [`optics`](Self::optics), the
 * focal lengths, skew and sensor tilt are the equivalent pinhole the optics give.
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
   * Beam-waist `1/e²` radius, in metres.
   */
  beam_waist_m: number;
  /**
   * The camera that images this laser's line, in a rig with one laser
   * per camera: its laser images and laser-plane calibration use this
   * laser only. Lasers that name cameras must name different ones.
   */
  camera?: string | null;
  /**
   * Fan half-angle in radians, in `(0, π/2)`.
   */
  fan_half_angle: number;
  /**
   * Fan radius, in metres: how far the fan reaches from the laser, the
   * same along every ray.
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
   * Optical power in watts, as the camera responds to it (its spectral
   * response at the wavelength folded in), spread evenly over the fan's
   * angle. Only laser images use it; absent, they take etendue-synth's
   * default (0.5 mW).
   */
  power_w?: number | null;
  /**
   * Distance from the laser origin to the waist along each ray, metres:
   * where a focusable laser is focused. Absent, the waist is at the laser.
   */
  waist_distance_m?: number | null;
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
   * The layout its cameras and lasers were made from, kept so it can be
   * edited again (`RigLayout::expand`). The cameras and lasers are the
   * source of truth; this is not checked against them.
   */
  layout?: RigLayout | null;
  /**
   * Parent frame (e.g. `"world"` or `"<robot>/tool0"` for eye-in-hand).
   */
  parent: string;
  parent_se3_self: Iso3Schema;
}
/**
 * How a rig's cameras (and lasers) are laid out.
 */
export interface RigLayout {
  camera: CameraTemplate;
  /**
   * Number of cameras: 1 (mono), 2 (stereo), 3, 6, …
   */
  count: number;
  /**
   * Which way each camera's image +X points.
   */
  image_x?: "tangential" | "radial" | "common";
  /**
   * One laser per camera, if any.
   */
  lasers?: LaserLayout | null;
  /**
   * Azimuth of camera 0 about the rig axis, radians.
   */
  phase_rad?: number;
  /**
   * Distance of each camera from the rig axis, metres (`0` only for one
   * camera).
   */
  radius_m: number;
  /**
   * Angle between each camera's axis and the rig axis, radians; default
   * `atan(radius / working_distance)`.
   */
  tilt_rad?: number | null;
  /**
   * Where the camera axes meet the rig axis (with the default tilt), and
   * where the lasers aim: `(0, 0, working_distance_m)` in the rig frame.
   */
  working_distance_m: number;
}
/**
 * The camera every position gets.
 */
export interface CameraTemplate {
  /**
   * The cameras' physical optics, if they have them (ADR 0008).
   */
  optics?: OpticsSpec | null;
  params: CameraParams;
  /**
   * Image size in pixels, `[width, height]`.
   *
   * @minItems 2
   * @maxItems 2
   */
  resolution: [number, number];
}
/**
 * One line laser per camera.
 */
export interface LaserLayout {
  /**
   * Azimuth of laser `i` relative to camera `i`, radians (unused with `camera_se3_laser`).
   */
  azimuth_rad?: number;
  /**
   * Beam-waist `1/e²` radius, metres.
   */
  beam_waist_m: number;
  /**
   * Each laser fixed to its camera's body at this pose, as a head carries it; otherwise it
   * stands on the ring, aimed at the working point.
   */
  camera_se3_laser?: Iso3Schema | null;
  /**
   * Fan half-angle, radians, in `(0, π/2)`.
   */
  fan_half_angle: number;
  /**
   * Fan radius, metres: how far the fan reaches from the laser, the same
   * along every ray.
   */
  fan_length: number;
  /**
   * Optical power, watts (`LaserSpec::power_w`).
   */
  power_w?: number | null;
  /**
   * Distance of each laser from the rig axis, metres (unused with `camera_se3_laser`).
   */
  radius_m?: number;
  /**
   * Distance from the laser origin to the waist along each ray, metres:
   * where a focusable laser is focused. Absent, the waist is at the laser.
   */
  waist_distance_m?: number | null;
  /**
   * Wavelength, nanometres.
   */
  wavelength_nm: number;
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
         * The print.
         */
        target:
          | {
              /**
               * Inner corner columns.
               */
              inner_cols: number;
              /**
               * Inner corner rows.
               */
              inner_rows: number;
              /**
               * White inset in the black squares, as a fraction of the side.
               */
              inner_square_rel?: number | null;
              kind: "chessboard";
              /**
               * Square side, mm.
               */
              square_size_mm: number;
            }
          | {
              /**
               * Marker border width in bits.
               */
              border_bits?: number;
              /**
               * Square columns.
               */
              cols: number;
              /**
               * ArUco dictionary, e.g. `"DICT_4X4_50"`.
               */
              dictionary: string;
              /**
               * White inset in the plain black squares, as a fraction of the side.
               */
              inner_square_rel?: number | null;
              kind: "charuco";
              /**
               * Marker placement; `opencv_charuco` (OpenCV's modern layout).
               */
              marker_layout?: "opencv_charuco";
              /**
               * Marker side as a fraction of the square side.
               */
              marker_size_rel: number;
              /**
               * Square rows.
               */
              rows: number;
              /**
               * Square side, mm.
               */
              square_size_mm: number;
            }
          | {
              /**
               * Circle diameter as a fraction of the square side.
               */
              circle_diameter_rel?: number;
              /**
               * The three circles.
               *
               * @minItems 3
               * @maxItems 3
               */
              circles: [MarkerCircle, MarkerCircle, MarkerCircle];
              /**
               * Inner corner columns.
               */
              inner_cols: number;
              /**
               * Inner corner rows.
               */
              inner_rows: number;
              /**
               * White inset in the black squares, as a fraction of the side.
               */
              inner_square_rel?: number | null;
              kind: "marker_board";
              /**
               * Square side, mm.
               */
              square_size_mm: number;
            }
          | {
              /**
               * Square columns.
               */
              cols: number;
              /**
               * Edge-dot diameter as a fraction of the square side.
               */
              dot_diameter_rel?: number;
              kind: "puzzle_board";
              /**
               * Master column of the board's first square.
               */
              origin_col?: number;
              /**
               * Master row of the board's first square.
               */
              origin_row?: number;
              /**
               * Square rows.
               */
              rows: number;
              /**
               * Square side, mm.
               */
              square_size_mm: number;
            }
          | {
              /**
               * Squares along the cylinder axis.
               */
              axial_squares: number;
              /**
               * Master column the strip is cut from.
               */
              axial_start_col?: number;
              /**
               * Squares around the circumference: a supported period (12, 18,
               * 24, 30, 36 or 42).
               */
              circumference_squares: number;
              /**
               * Edge-dot diameter as a fraction of the square side.
               */
              dot_diameter_rel?: number;
              kind: "puzzlepole";
              /**
               * Square side, mm.
               */
              square_size_mm: number;
              /**
               * Master row the strip is cut from (the seam); must match the
               * period (see calib-targets `PuzzlePolePeriod`).
               */
              start_row: number;
            };
        type: "printed";
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
/**
 * A marker board circle: on square `(i, j)` (column, row), white or black.
 */
export interface MarkerCircle {
  /**
   * Square column.
   */
  i: number;
  /**
   * Square row.
   */
  j: number;
  /**
   * Disk colour.
   */
  polarity: "white" | "black";
}

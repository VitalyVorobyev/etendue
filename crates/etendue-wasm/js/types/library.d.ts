/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * One library item. Exactly one of the part fields is set: it names the item's kind.
 */
export interface LibraryItem {
  /**
   * A camera design: optics (when physical), the as-built model and the image size.
   */
  camera?: CameraTemplate | null;
  /**
   * More about it.
   */
  description?: string | null;
  /**
   * A laser-triangulation head.
   */
  head?: HeadDesign | null;
  /**
   * The item's id: its file stem.
   */
  id: string;
  /**
   * A line laser.
   */
  laser?: LaserPart | null;
  /**
   * A lens.
   */
  lens?: LensPart | null;
  /**
   * What it is, for people.
   */
  name: string;
  /**
   * A rig design.
   */
  rig?: RigDesign | null;
  /**
   * A sensor.
   */
  sensor?: SensorPart | null;
  /**
   * A calibration target.
   */
  target?: TargetPart | null;
  /**
   * Format version, [`LIBRARY_VERSION`].
   */
  version: number;
}
/**
 * The camera model of every position of a layout.
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
 * Projection, distortion, sensor and intrinsics.
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
 * A laser-triangulation head: a camera design and a line laser fixed to the camera's body.
 */
export interface HeadDesign {
  camera: CameraTemplate;
  camera_se3_laser: Iso3Schema;
  /**
   * How far the fan reaches along its edge rays, metres.
   */
  fan_length_m: number;
  laser: LaserPart;
  /**
   * Where it must work.
   */
  window?: HeadWindow | null;
}
/**
 * The laser's pose in the camera's body frame (its fan in the laser's `y–z` plane,
 * opening about `+z`).
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
 * The laser.
 */
export interface LaserPart {
  /**
   * Beam-waist `1/e²` radius across the line, metres, when known.
   */
  beam_waist_m?: number | null;
  /**
   * Fan half-angle, radians.
   */
  fan_half_angle: number;
  /**
   * Optical power, milliwatts.
   */
  power_mw?: number | null;
  /**
   * Emission wavelength, nanometres.
   */
  wavelength_nm: number;
}
/**
 * Where a head must work: a window in its laser plane, measured from the laser's origin.
 */
export interface HeadWindow {
  /**
   * Farthest range, metres.
   */
  far_m: number;
  /**
   * The largest acceptable blur (geometric and diffraction), pixels.
   */
  max_blur_px?: number | null;
  /**
   * Nearest range along the laser's central ray, metres.
   */
  near_m: number;
  /**
   * Width along the laser line, centred on the central ray, metres.
   */
  width_m: number;
}
/**
 * A lens: what a datasheet gives, and the thick-lens data the camera model needs.
 */
export interface LensPart {
  /**
   * Signed distance from H to the entrance pupil, metres (`0` when unknown).
   */
  entrance_pupil_m?: number;
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
   * Diagonal field of view on the sensor it was specified for, degrees.
   */
  fov_diagonal_deg?: number | null;
  /**
   * The widest aperture: the smallest f-number.
   */
  min_f_number: number;
  /**
   * The mount.
   */
  mount: "c_mount" | "cs_mount" | "f_mount" | "m12" | "other";
  /**
   * Spatial frequency at 50 % MTF, line pairs per millimetre.
   */
  mtf50_lp_mm?: number | null;
  /**
   * Signed distance from H′ to H, metres (`0` when unknown: a thin lens).
   */
  principal_gap_m?: number;
}
/**
 * A rig of identical units.
 */
export interface RigDesign {
  layout: RigLayout;
}
/**
 * The layout, its camera (and lasers) included.
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
 * One line laser per camera.
 */
export interface LaserLayout {
  /**
   * Azimuth of laser `i` relative to camera `i`, radians (unused with `camera_se3_laser`).
   */
  azimuth_rad?: number;
  /**
   * Beam-waist `1/e²` radius at the laser, metres.
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
   * Fan reach along its central ray, metres.
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
   * Wavelength, nanometres.
   */
  wavelength_nm: number;
}
/**
 * An image sensor.
 */
export interface SensorPart {
  /**
   * Peak quantum efficiency, in `[0, 1]`.
   */
  peak_qe?: number | null;
  /**
   * Pixel pitch, metres.
   */
  pixel_pitch_m: number;
  /**
   * Read noise (RMS), electrons.
   */
  read_noise_e?: number | null;
  /**
   * Active pixels, `[width, height]`.
   *
   * @minItems 2
   * @maxItems 2
   */
  resolution: [number, number];
  /**
   * Full-well capacity, electrons.
   */
  well_depth_e?: number | null;
}
/**
 * A calibration target.
 */
export interface TargetPart {
  /**
   * What it is, as a scene target holds it.
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

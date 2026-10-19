/**
 * DO NOT EDIT: generated from schemas/*.schema.json by `bun run generate:types` (in web/).
 * The source of truth is the `etendue-scene` Rust types.
 */

/**
 * `etendue-project.json`.
 */
export interface ProjectManifest {
  /**
   * The cell folders, each holding `scene.json` and usually `scenario.json`.
   */
  cells?: string[];
  defaults?: ProjectDefaults;
  /**
   * What the project is.
   */
  name: string;
  /**
   * Robots that come with the project: folders holding `robot.json`.
   */
  robots?: string[];
  /**
   * Format version, [`PROJECT_VERSION`].
   */
  version: number;
}
/**
 * What new things are made from.
 */
export interface ProjectDefaults {
  /**
   * A camera design (`cameras/<id>`).
   */
  camera?: string | null;
  /**
   * A laser part (`lasers/<id>`).
   */
  laser?: string | null;
  /**
   * A target part (`targets/<id>`).
   */
  target?: string | null;
}

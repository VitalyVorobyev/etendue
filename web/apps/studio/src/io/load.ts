/**
 * Load a scene with everything it references: the robot manifests (relative to the scene)
 * and their URDFs and meshes (relative to each manifest). Validation and baking are the
 * kernel's (`@etendue/wasm`); this only finds and reads files.
 */

import type { RobotManifest, RobotSource, ScenarioSpec, SceneSpec } from "@etendue/wasm";

import { dirname, join } from "./paths";
import type { FileSource } from "./source";

/** A scene robot's assets. */
export interface LoadedRobot extends RobotSource {
  /** Where the manifest was found (for messages). */
  manifestPath: string;
  /** URL of a manifest-relative mesh path. */
  meshUrl: (meshPath: string) => string;
}

/** A scene and everything it needs, read but not yet validated. */
export interface LoadedScene {
  /** Where it came from, e.g. `examples/eye_in_hand_ur5e`. */
  label: string;
  scene: SceneSpec;
  /** `null` when no scenario was given: the rest pose is shown. */
  scenario: ScenarioSpec | null;
  /**
   * The scene file's absolute path, when the source is the filesystem (the Tauri shell).
   * The native pipeline (dataset generation) needs it; dropped files have none.
   */
  origin: string | null;
  robots: LoadedRobot[];
}

/**
 * Where a robot manifest path from a scene may also be found in the repository's robot
 * library: `…/assets/robots/<id>/robot.json` → `assets/robots/<id>/robot.json`.
 */
export function libraryPath(manifestPath: string): string | null {
  const m = /(?:^|\/)(assets\/robots\/[^/]+\/[^/]+)$/.exec(manifestPath);
  if (m) return m[1]!;
  const dir = dirname(manifestPath).split("/").at(-1);
  return dir ? `assets/robots/${dir}/robot.json` : null;
}

async function parse<T>(source: FileSource, path: string): Promise<T> {
  const text = await source.readText(path);
  try {
    return JSON.parse(text) as T;
  } catch (e) {
    throw new Error(`${source.name}: ${path}: ${e instanceof Error ? e.message : String(e)}`);
  }
}

/**
 * Read the scene at `scenePath` in `source` and its robots' assets. A robot manifest that is
 * not in `source` is looked up in `library` (the repository's `assets/robots/`), so a scene
 * dropped on its own still finds the robots it names.
 */
export async function loadScene(
  source: FileSource,
  scenePath: string,
  scenarioPath: string | null,
  library?: FileSource,
): Promise<LoadedScene> {
  const scene = await parse<SceneSpec>(source, scenePath);
  const scenario = scenarioPath === null ? null : await parse<ScenarioSpec>(source, scenarioPath);
  const robots = await Promise.all(
    (scene.robots ?? []).map(async (robot): Promise<LoadedRobot> => {
      const wanted = join(dirname(scenePath), robot.manifest);
      let from = source;
      let manifestPath = wanted;
      if (!(await source.has(wanted))) {
        const fallback = libraryPath(wanted);
        if (library && fallback !== null && (await library.has(fallback))) {
          from = library;
          manifestPath = fallback;
        } else {
          throw new Error(`robot \`${robot.id}\`: manifest ${robot.manifest} not found in ${source.name}`);
        }
      }
      const manifest = await parse<RobotManifest>(from, manifestPath);
      const dir = dirname(manifestPath);
      const urdf = await from.readText(join(dir, manifest.urdf));
      return {
        id: robot.id,
        manifest,
        urdf,
        manifestPath: `${from.name}: ${manifestPath}`,
        meshUrl: (meshPath) => from.url(join(dir, meshPath)),
      };
    }),
  );
  return {
    label: scenePath.replace(/\/[^/]*$/, ""),
    scene,
    scenario,
    robots,
    origin: source.absolute?.(scenePath) ?? null,
  };
}

/** What a dropped JSON document is, judged by its fields. */
export function documentKind(doc: unknown): "scene" | "scenario" | "baked" | "manifest" | null {
  if (typeof doc !== "object" || doc === null) return null;
  const d = doc as Record<string, unknown>;
  if (Array.isArray(d.samples) && Array.isArray(d.frames)) return "baked";
  if (Array.isArray(d.steps)) return "scenario";
  if (typeof d.urdf === "string" && Array.isArray(d.joints)) return "manifest";
  if (typeof d.version === "number" && ["robots", "cameras", "rigs", "targets", "frames"].some((k) => k in d)) {
    return "scene";
  }
  return null;
}

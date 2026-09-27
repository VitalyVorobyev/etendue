import {
  type MeshLoader,
  type RobotVisual,
  applyRobotMaterial,
  attachRobotVisuals,
  disposeObject,
  loadRobotVisuals,
} from "@vitavision/three";
import { useEffect, useRef, useState } from "react";
import type { MeshStandardMaterial, Object3D } from "three";

import { useSceneColors } from "./colors";
import { AtFrame, useFrameTree } from "./FrameTree";
import { FrameAxes } from "./gizmos";

/** Props of {@link Robot}. */
export interface RobotProps {
  /** Scene robot id: its links are frames `"<id>/<link>"`. */
  id: string;
  /** The manifest's `visuals`. */
  visuals: readonly RobotVisual[];
  /** Maps a manifest-relative mesh path to a URL. */
  resolve: (meshPath: string) => string;
  /** Mesh loader (default: glTF). */
  loader?: MeshLoader;
  /** Links to draw axes for, e.g. the TCP; links whose mesh failed get axes too. */
  axes?: readonly string[];
  /** Called once loading settles with the links whose mesh failed. */
  onLoaded?: ((failed: { link: string; error: string }[]) => void) | undefined;
}

/**
 * A robot's link meshes, attached to its link frames in the enclosing `FrameTree`, in one
 * matte material from the `fg-muted` token.
 */
export function Robot({ id, visuals, resolve, loader, axes = [], onLoaded }: RobotProps) {
  const runtime = useFrameTree();
  const colors = useSceneColors();
  const [state, setState] = useState<{ material: MeshStandardMaterial | null; failed: string[] }>({
    material: null,
    failed: [],
  });
  const onLoadedRef = useRef(onLoaded);
  useEffect(() => {
    onLoadedRef.current = onLoaded;
  }, [onLoaded]);

  useEffect(() => {
    let cancelled = false;
    let meshes: Map<string, Object3D> | undefined;
    let material: MeshStandardMaterial | undefined;
    void loadRobotVisuals(visuals, resolve, loader).then((result) => {
      if (cancelled) {
        for (const m of result.meshes.values()) disposeObject(m);
        return;
      }
      meshes = result.meshes;
      attachRobotVisuals(runtime, id, meshes);
      // Built neutral; the effect below applies the theme colour (and follows it).
      material = applyRobotMaterial(meshes.values(), "gray");
      setState({ material, failed: result.failed.map((f) => f.link) });
      onLoadedRef.current?.(result.failed);
    });
    return () => {
      cancelled = true;
      for (const m of meshes?.values() ?? []) {
        m.removeFromParent();
        disposeObject(m);
      }
      material?.dispose();
    };
  }, [runtime, id, visuals, resolve, loader]);

  useEffect(() => {
    state.material?.color.set(colors.muted);
  }, [state.material, colors.muted]);

  return (
    <>
      {[...new Set([...axes, ...state.failed])].map((link) => (
        <AtFrame key={link} name={`${id}/${link}`}>
          <FrameAxes size={0.06} />
        </AtFrame>
      ))}
    </>
  );
}

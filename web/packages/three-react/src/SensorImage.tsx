import { type CanonicalPinhole, type FrameTreeRuntime, type RemapTable, SensorView } from "@vitavision/three";
import { useEffect, useRef } from "react";
import { WebGLRenderer } from "three";

import { useSceneColors } from "./colors";
import type { PlayheadSource } from "./FrameTree";

/** Props of {@link SensorImage}. */
export interface SensorImageProps {
  /** The runtime of the scene's `FrameTree` (its `onRuntime`). */
  runtime: FrameTreeRuntime;
  /** The camera's frame name (a CV camera frame). */
  frame: string;
  /** The camera's canonical pinhole. */
  canonical: CanonicalPinhole;
  /** The camera's remap LUT; the canvas is `lut.width × lut.height` pixels. */
  lut: RemapTable;
  /** Which sample to show, read every animation frame. */
  playhead: PlayheadSource;
  /** Class of the canvas (size it with CSS; its pixel size is the LUT's). */
  className?: string;
  /** Accessible name. */
  label?: string;
}

/**
 * A calibrated camera's image of the scene in the enclosing `FrameTree`: a
 * `SensorView` (`@vitavision/three`) on a canvas of its own, rendered from the same objects (physical layer
 * only) at the playhead. Redraws when the playhead moves, and a few times a second otherwise
 * so late-loading meshes and theme changes appear.
 */
export function SensorImage({ runtime, frame, canonical, lut, playhead, className, label }: SensorImageProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const colors = useSceneColors();
  const background = colors.canvas;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const renderer = new WebGLRenderer({ canvas, antialias: false });
    renderer.setPixelRatio(1);
    renderer.setSize(lut.width, lut.height, false);
    renderer.setClearColor(background);
    const view = new SensorView({ canonical, lut });
    view.setBackground(background);
    let raf = 0;
    let last = -1;
    let lastAt = 0;
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const k = playhead.get();
      if (k === last && now - lastAt < 250) return;
      last = k;
      lastAt = now;
      runtime.apply(k);
      const scene = runtime.root.parent ?? runtime.root;
      scene.updateMatrixWorld();
      const camera = runtime.frame(frame);
      if (camera) view.render(renderer, scene, camera.matrixWorld);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      view.dispose();
      renderer.dispose();
    };
  }, [runtime, frame, canonical, lut, playhead, background]);

  return (
    <canvas
      ref={canvasRef}
      width={lut.width}
      height={lut.height}
      className={className}
      role="img"
      aria-label={label ?? `${frame} sensor image`}
    />
  );
}

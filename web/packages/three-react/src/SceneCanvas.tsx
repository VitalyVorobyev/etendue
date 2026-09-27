import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { Z_UP } from "@vitavision/three";
import { type ReactNode, useEffect, useMemo } from "react";
import { GridHelper, Vector3 } from "three";
import { OrbitControls as ThreeOrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

import { useSceneColors } from "./colors";

/** Props of {@link SceneCanvas}. */
export interface SceneCanvasProps {
  /** Scene content. */
  children?: ReactNode;
  /** Initial eye position in world coordinates (metres). */
  eye?: readonly [number, number, number];
  /** Initial orbit target. */
  target?: readonly [number, number, number];
  /** Ground grid size in metres, `0` for none. */
  grid?: number;
  /** Called when a click hits nothing. */
  onPointerMissed?: (() => void) | undefined;
  /** Class of the wrapping element (size it; the canvas fills it). */
  className?: string;
  /** Accessible label of the canvas. */
  label?: string;
}

/**
 * A Z-up 3D viewport in vitavision colours: orbit controls, a ground grid in the world XY
 * plane, and ambient plus key lighting. Everything inside is in world coordinates (metres).
 */
export function SceneCanvas({
  children,
  eye = [1.6, -1.4, 1.1],
  target = [0.3, 0, 0.3],
  grid = 2,
  onPointerMissed,
  className,
  label = "3D scene",
}: SceneCanvasProps) {
  const colors = useSceneColors();
  return (
    <div className={className} role="img" aria-label={label}>
      <Canvas
        camera={{ position: [...eye], up: [Z_UP.x, Z_UP.y, Z_UP.z], near: 0.01, far: 100, fov: 45 }}
        dpr={[1, 2]}
        {...(onPointerMissed ? { onPointerMissed } : {})}
        style={{ background: colors.background }}
      >
        <ambientLight intensity={1.2} />
        <directionalLight position={[2, -3, 4]} intensity={1.8} />
        {grid > 0 && <Grid size={grid} />}
        <OrbitControls target={target} />
        {children}
      </Canvas>
    </div>
  );
}

function Grid({ size }: { size: number }) {
  const colors = useSceneColors();
  const grid = useMemo(() => {
    const g = new GridHelper(size, Math.round(size * 10), colors.lineStrong, colors.line);
    g.rotation.x = Math.PI / 2; // GridHelper lies in XZ; the ground is XY.
    return g;
  }, [size, colors.line, colors.lineStrong]);
  useEffect(() => () => grid.dispose(), [grid]);
  return <primitive object={grid} />;
}

function OrbitControls({ target }: { target: readonly [number, number, number] }) {
  const camera = useThree((s) => s.camera);
  const element = useThree((s) => s.gl.domElement);
  const controls = useMemo(() => {
    const c = new ThreeOrbitControls(camera, element);
    c.enableDamping = true;
    return c;
  }, [camera, element]);
  const [tx, ty, tz] = target;
  useEffect(() => {
    controls.target.copy(new Vector3(tx, ty, tz));
    controls.update();
  }, [controls, tx, ty, tz]);
  useEffect(() => () => controls.dispose(), [controls]);
  useFrame(() => controls.update());
  return null;
}

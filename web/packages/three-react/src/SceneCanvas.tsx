import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { GIZMO_LAYER, Z_UP, setLayer } from "@vitavision/three";
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
  /** The world's up axis. Default +Z (etendue); `[0, -1, 0]` suits a camera-frame (CV) scene. */
  up?: readonly [number, number, number];
  /** Vertical field of view in degrees. Default 45. */
  fov?: number;
  /** Near and far clip distances in metres. Default `[0.01, 100]`. */
  clip?: readonly [number, number];
  /** Ground grid size in metres, `0` for none. The grid lies in the plane normal to `up`. */
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
  up = [Z_UP.x, Z_UP.y, Z_UP.z],
  fov = 45,
  clip = [0.01, 100],
  grid = 2,
  onPointerMissed,
  className,
  label = "3D scene",
}: SceneCanvasProps) {
  const colors = useSceneColors();
  return (
    <div className={className} role="img" aria-label={label}>
      <Canvas
        camera={{ position: [...eye], up: [...up], near: clip[0], far: clip[1], fov }}
        dpr={[1, 2]}
        {...(onPointerMissed ? { onPointerMissed } : {})}
        style={{ background: colors.background }}
        onCreated={({ camera, raycaster }) => {
          // The viewport shows gizmos and picks through them; sensor views see only the world.
          camera.layers.enable(GIZMO_LAYER);
          raycaster.layers.enable(GIZMO_LAYER);
        }}
      >
        <ambientLight intensity={1.2} />
        <directionalLight position={[2, -3, 4]} intensity={1.8} />
        {grid > 0 && <Grid size={grid} up={up} />}
        <OrbitControls target={target} />
        {children}
      </Canvas>
    </div>
  );
}

function Grid({ size, up }: { size: number; up: readonly [number, number, number] }) {
  const colors = useSceneColors();
  const [ux, uy, uz] = up;
  const grid = useMemo(() => {
    const g = new GridHelper(size, Math.round(size * 10), colors.lineStrong, colors.line);
    // GridHelper lies in the XZ plane (normal +Y); turn its normal onto `up`.
    g.quaternion.setFromUnitVectors(new Vector3(0, 1, 0), new Vector3(ux, uy, uz).normalize());
    setLayer(g, GIZMO_LAYER);
    return g;
  }, [size, ux, uy, uz, colors.line, colors.lineStrong]);
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

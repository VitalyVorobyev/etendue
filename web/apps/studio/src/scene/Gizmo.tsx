import type { Iso3Schema } from "@etendue/wasm";
import { useFrame, useThree } from "@react-three/fiber";
import { GIZMO_LAYER, type Iso3, composeIso3, invertIso3, iso3FromMatrix, setLayer } from "@vitavision/three";
import { useEffect, useRef } from "react";
import { Object3D, Vector3 } from "three";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";

import type { PoseSource } from "../kernel/posed";

/** How the gizmo moves its frame. */
export interface GizmoOptions {
  mode: "translate" | "rotate";
  /** Axes of the frame itself, or of the world. */
  space: "local" | "world";
  /** Snap to 10 mm and 5°. */
  snap: boolean;
}

const SNAP_M = 0.01;
const SNAP_RAD = (5 * Math.PI) / 180;

/**
 * A translate or rotate handle on frame `frame`, which moves the entity placed in `parent` at
 * `placement`. A drag reports the new placement through `onPreview` as it goes, and `onCommit`
 * on release; Escape during a drag calls `onCancel`.
 *
 * The drag moves `frame` rigidly; the placement follows by the same motion:
 * `P = W_parent⁻¹ · W_frame · W_frame,0⁻¹ · W_parent · P_0`. For an entity `frame` is its own
 * frame; for a robot it is the base link, rigidly attached to what the placement places.
 *
 * The handle takes the pointer before the viewport's orbit controls (capture-phase listeners on
 * the canvas's container): a press on a handle never orbits and never clears the selection.
 */
export function Gizmo({
  frame,
  parent,
  placement,
  poses,
  options,
  onPreview,
  onCommit,
  onCancel,
}: {
  frame: string;
  parent: string;
  /** The placement as currently shown. */
  placement: Iso3Schema;
  /** World poses at the playhead, an edit in progress included. */
  poses: PoseSource;
  options: GizmoOptions;
  onPreview: (placement: Iso3Schema) => void;
  onCommit: () => void;
  onCancel: () => void;
}) {
  const camera = useThree((s) => s.camera);
  const canvas = useThree((s) => s.gl.domElement);
  const scene = useThree((s) => s.scene);
  const props = useRef({ frame, parent, placement, poses, options, onPreview, onCommit, onCancel });
  useEffect(() => {
    props.current = { frame, parent, placement, poses, options, onPreview, onCommit, onCancel };
  });
  const state = useRef<{ controls: TransformControls; proxy: Object3D; dragging: boolean; host: HTMLElement } | null>(
    null,
  );

  useEffect(() => {
    const proxy = new Object3D();
    const controls = new TransformControls(camera);
    controls.setSize(0.8);
    controls.attach(proxy);
    const helper = controls.getHelper();
    // Drawn and picked in the viewport only, never in a sensor image.
    setLayer(helper, GIZMO_LAYER);
    controls.getRaycaster().layers.enable(GIZMO_LAYER);
    scene.add(proxy, helper);
    const host = canvas.parentElement ?? canvas;
    const shared = { controls, proxy, dragging: false, host };
    state.current = shared;

    let drag: { frame0: Iso3; parent0: Iso3; placement0: Iso3Schema } | null = null;
    let swallowClick = false;
    // Normalised device coordinates. TransformControls reads only `x`, `y` and `button` of the
    // "pointer" it is given, although its types ask for a whole PointerEvent.
    const pointer = (e: PointerEvent): PointerEvent => {
      const r = canvas.getBoundingClientRect();
      const ndc = {
        x: ((e.clientX - r.left) / r.width) * 2 - 1,
        y: -((e.clientY - r.top) / r.height) * 2 + 1,
        button: e.button,
      };
      return ndc as unknown as PointerEvent;
    };
    const end = () => {
      drag = null;
      shared.dragging = false;
      swallowClick = true;
    };
    const onDown = (e: PointerEvent) => {
      const p = pointer(e);
      controls.pointerHover(p);
      if (controls.axis === null || e.button !== 0) return;
      const { poses: source, frame: f, parent: par, placement: start } = props.current;
      const frame0 = source.pose(f);
      const parent0 = source.pose(par);
      if (!frame0 || !parent0) return;
      e.stopPropagation();
      drag = { frame0, parent0, placement0: start };
      shared.dragging = true;
      controls.pointerDown(p);
      host.setPointerCapture(e.pointerId);
    };
    const onMove = (e: PointerEvent) => {
      if (drag) {
        e.stopPropagation();
        controls.pointerMove(pointer(e));
      } else {
        controls.pointerHover(pointer(e));
      }
    };
    const onUp = (e: PointerEvent) => {
      if (!drag) return;
      e.stopPropagation();
      controls.pointerUp(pointer(e));
      if (host.hasPointerCapture(e.pointerId)) host.releasePointerCapture(e.pointerId);
      end();
      props.current.onCommit();
    };
    // The click that ends a drag is not a click on empty space.
    const onClick = (e: MouseEvent) => {
      if (!swallowClick) return;
      swallowClick = false;
      e.stopPropagation();
    };
    const onKey = (e: KeyboardEvent) => {
      if (!drag || e.key !== "Escape") return;
      controls.pointerUp(null);
      end();
      swallowClick = false;
      props.current.onCancel();
    };
    controls.addEventListener("objectChange", () => {
      if (!drag) return;
      proxy.updateMatrixWorld();
      const moved = iso3FromMatrix(proxy.matrixWorld);
      const { frame0, parent0, placement0 } = drag;
      props.current.onPreview(
        composeIso3(
          composeIso3(composeIso3(invertIso3(parent0), moved), invertIso3(frame0)),
          composeIso3(parent0, placement0),
        ),
      );
    });
    host.addEventListener("pointerdown", onDown, true);
    host.addEventListener("pointermove", onMove, true);
    host.addEventListener("pointerup", onUp, true);
    host.addEventListener("click", onClick, true);
    window.addEventListener("keydown", onKey);
    return () => {
      host.removeEventListener("pointerdown", onDown, true);
      host.removeEventListener("pointermove", onMove, true);
      host.removeEventListener("pointerup", onUp, true);
      host.removeEventListener("click", onClick, true);
      window.removeEventListener("keydown", onKey);
      scene.remove(proxy, helper);
      controls.detach();
      // Not `controls.dispose()`: it disconnects from a DOM element these controls never had.
      helper.dispose();
      delete host.dataset.gizmoAt;
      state.current = null;
    };
  }, [camera, canvas, scene]);

  // Follow the frame (playback, other edits) and the options, except during a drag. The handle's
  // place on screen (CSS px in the canvas) is published as `data-gizmo-at`, for tests.
  const onScreen = useRef(new Vector3());
  useFrame(() => {
    const s = state.current;
    if (!s || s.dragging) return;
    const { poses: source, frame: f, options: o } = props.current;
    const pose = source.pose(f);
    s.controls.getHelper().visible = pose !== undefined;
    if (!pose) return;
    s.proxy.position.set(pose.translation[0], pose.translation[1], pose.translation[2]);
    s.proxy.quaternion.set(pose.rotation[0], pose.rotation[1], pose.rotation[2], pose.rotation[3]);
    s.controls.setMode(o.mode);
    s.controls.setSpace(o.space);
    s.controls.setTranslationSnap(o.snap ? SNAP_M : null);
    s.controls.setRotationSnap(o.snap ? SNAP_RAD : null);
    const v = onScreen.current.copy(s.proxy.position).project(camera);
    const at = `${(((v.x + 1) / 2) * canvas.clientWidth).toFixed(0)},${(((1 - v.y) / 2) * canvas.clientHeight).toFixed(0)}`;
    if (s.host.dataset.gizmoAt !== at) s.host.dataset.gizmoAt = at;
  });
  return null;
}

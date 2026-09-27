/**
 * A calibrated camera's image in the browser (etendue ADR 0004, PLAN P2-2): render the
 * canonical pinhole into a render target, then resample it through the remap LUT, so the
 * result carries the target camera's distortion, skew, principal point and Scheimpflug
 * geometry — with no camera-model math in the shader. The LUT comes from the host (e.g.
 * `@etendue/wasm` `remap`).
 */

import {
  type ColorRepresentation,
  DataTexture,
  FloatType,
  GLSL3,
  HalfFloatType,
  LinearFilter,
  Mesh,
  NearestFilter,
  OrthographicCamera,
  PerspectiveCamera,
  PlaneGeometry,
  RGFormat,
  Scene,
  ShaderMaterial,
  Color,
  type WebGLRenderer,
  WebGLRenderTarget,
  type Matrix4,
  type Object3D,
} from "three";

import { glCameraMatrix } from "./conventions";
import { PHYSICAL_LAYER } from "./layers";

/** Which image coordinate names a pixel's centre (decided by etendue probe P4-2). */
export type PixelCentre = "integer" | "half";

/** The canonical pinhole: centred, square pixels. */
export interface CanonicalPinhole {
  /** Width in canonical pixels. */
  width: number;
  /** Height in canonical pixels. */
  height: number;
  /** Focal length in canonical pixels (`fx = fy`). */
  focalPx: number;
}

/** A remap LUT: for each target pixel, the canonical coordinate to sample. */
export interface RemapTable {
  /** Target width in pixels. */
  width: number;
  /** Target height in pixels. */
  height: number;
  /** Row-major (top row first), two floats per pixel; `NaN` where there is nothing to sample. */
  data: Float32Array;
  /** The convention both the target pixels and the canonical coordinates use. */
  pixelCentre: PixelCentre;
}

/** Options of {@link SensorView}. */
export interface SensorViewOptions {
  /** The canonical pinhole the scene is rendered with before the remap. */
  canonical: CanonicalPinhole;
  /** The remap LUT from target pixels to canonical coordinates; it sets the output size. */
  lut: RemapTable;
  /** Near and far clip distances in metres. Default `[0.01, 50]`. */
  clip?: readonly [number, number];
  /** MSAA samples of the canonical render. Default 4. */
  samples?: number;
  /**
   * Storage of the canonical render: `"byte"` (8-bit, the default, for display) or
   * `"half"` (16-bit float, linear radiance without quantisation, for measurement).
   */
  precision?: "byte" | "half";
  /**
   * Resampling: `taps × taps` bilinear samples averaged over each output pixel's footprint
   * in the canonical image (a box filter; the footprint comes from the LUT's own finite
   * differences). Use about the supersampling factor; `1` (the default) point-samples, which
   * aliases a supersampled canonical render.
   */
  taps?: number;
}

const VERTEX = /* glsl */ `
out vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}`;

const FRAGMENT = /* glsl */ `
precision highp float;
precision highp sampler2D;
uniform sampler2D canonical;
uniform sampler2D lut;
uniform vec2 canonicalSize;
uniform float edge;
uniform int lutHeight;
uniform vec3 background;
uniform int taps;
layout(location = 0) out highp vec4 pc_fragColor;
#define gl_FragColor pc_fragColor
vec2 entry(ivec2 ij) { return texelFetch(lut, ij, 0).rg; }
bool bad(vec2 v) { return isnan(v.x) || isnan(v.y); }
vec4 sampleAt(vec2 p) {
  // Canonical coordinate → texture coordinate; the render target's rows run bottom-up.
  return texture(canonical, vec2((p.x - edge) / canonicalSize.x, 1.0 - (p.y - edge) / canonicalSize.y));
}
void main() {
  // Target pixel (i, j), row 0 at the top; fragment rows count from the bottom.
  ivec2 ij = ivec2(int(gl_FragCoord.x), lutHeight - 1 - int(gl_FragCoord.y));
  vec2 p = entry(ij);
  if (bad(p)) {
    gl_FragColor = vec4(background, 1.0);
  } else if (taps <= 1) {
    gl_FragColor = sampleAt(p);
  } else {
    // The pixel's footprint in the canonical image: the LUT's finite differences.
    ivec2 size = textureSize(lut, 0);
    ivec2 l = max(ij - ivec2(1, 0), ivec2(0)), r = min(ij + ivec2(1, 0), size - 1);
    ivec2 t = max(ij - ivec2(0, 1), ivec2(0)), b = min(ij + ivec2(0, 1), size - 1);
    vec2 du = (entry(r) - entry(l)) / float(max(r.x - l.x, 1));
    vec2 dv = (entry(b) - entry(t)) / float(max(b.y - t.y, 1));
    if (bad(du)) du = vec2(0.0);
    if (bad(dv)) dv = vec2(0.0);
    vec4 acc = vec4(0.0);
    float n = float(taps);
    for (int y = 0; y < taps; y++) {
      for (int x = 0; x < taps; x++) {
        vec2 o = (vec2(float(x), float(y)) + 0.5) / n - 0.5;
        acc += sampleAt(p + o.x * du + o.y * dv);
      }
    }
    gl_FragColor = acc / (n * n);
  }
  #include <colorspace_fragment>
}`;

/**
 * Renders a scene as a calibrated camera sees it. Size the output (canvas or render target)
 * to exactly `lut.width × lut.height` pixels at pixel ratio 1: every output pixel is one LUT
 * entry.
 *
 * Only {@link PHYSICAL_LAYER} is rendered: gizmos stay out of the image.
 */
export class SensorView {
  /** The canonical camera (three.js, OpenGL axes). Its matrix is set by {@link SensorView.render}. */
  readonly camera: PerspectiveCamera;
  /** The canonical render. */
  readonly canonicalTarget: WebGLRenderTarget;
  readonly #lut: DataTexture;
  readonly #material: ShaderMaterial;
  readonly #quad: Mesh;
  readonly #quadScene = new Scene();
  readonly #quadCamera = new OrthographicCamera(-1, 1, 1, -1, 0, 1);

  constructor({ canonical, lut, clip = [0.01, 50], samples = 4, precision = "byte", taps = 1 }: SensorViewOptions) {
    if (lut.data.length !== 2 * lut.width * lut.height) {
      throw new Error(`LUT has ${lut.data.length} values for ${lut.width}×${lut.height} pixels`);
    }
    const fovY = (2 * Math.atan(canonical.height / 2 / canonical.focalPx) * 180) / Math.PI;
    this.camera = new PerspectiveCamera(fovY, canonical.width / canonical.height, clip[0], clip[1]);
    this.camera.matrixAutoUpdate = false;
    this.camera.layers.set(PHYSICAL_LAYER);

    this.canonicalTarget = new WebGLRenderTarget(canonical.width, canonical.height, {
      samples,
      ...(precision === "half" ? { type: HalfFloatType } : {}),
    });
    this.canonicalTarget.texture.minFilter = LinearFilter;
    this.canonicalTarget.texture.magFilter = LinearFilter;

    this.#lut = new DataTexture(lut.data, lut.width, lut.height, RGFormat, FloatType);
    this.#lut.minFilter = NearestFilter;
    this.#lut.magFilter = NearestFilter;
    this.#lut.needsUpdate = true;

    this.#material = new ShaderMaterial({
      glslVersion: GLSL3,
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      uniforms: {
        canonical: { value: this.canonicalTarget.texture },
        lut: { value: this.#lut },
        canonicalSize: { value: [canonical.width, canonical.height] },
        edge: { value: lut.pixelCentre === "integer" ? -0.5 : 0 },
        lutHeight: { value: lut.height },
        background: { value: new Color("black") },
        taps: { value: Math.max(1, Math.round(taps)) },
      },
      depthTest: false,
      depthWrite: false,
    });
    this.#quad = new Mesh(new PlaneGeometry(2, 2), this.#material);
    this.#quad.frustumCulled = false;
    this.#quadScene.add(this.#quad);
  }

  /** Colour of target pixels whose ray the canonical camera cannot image. */
  setBackground(color: ColorRepresentation): void {
    (this.#material.uniforms.background!.value as Color).set(color);
  }

  /**
   * Render `scene` from the calibrated camera at `worldSe3Cv` (a CV camera frame: +Z
   * forward, +Y down) into `target` (default: the renderer's canvas).
   */
  render(renderer: WebGLRenderer, scene: Object3D, worldSe3Cv: Matrix4, target: WebGLRenderTarget | null = null): void {
    glCameraMatrix(worldSe3Cv, this.camera.matrix);
    this.camera.matrixWorld.copy(this.camera.matrix);
    this.camera.matrixWorldInverse.copy(this.camera.matrix).invert();
    const previous = renderer.getRenderTarget();
    renderer.setRenderTarget(this.canonicalTarget);
    renderer.render(scene, this.camera);
    renderer.setRenderTarget(target);
    renderer.render(this.#quadScene, this.#quadCamera);
    renderer.setRenderTarget(previous);
  }

  /** Free the GPU resources. */
  dispose(): void {
    this.canonicalTarget.dispose();
    this.#lut.dispose();
    this.#material.dispose();
    this.#quad.geometry.dispose();
  }
}

# @vitavision/three-react

React Three Fiber components over `@vitavision/three`. Per-frame updates go through
`useFrame` and refs, never React state: playback is read from a `PlayheadSource`
(`{ get(): number }`, e.g. `@vitavision/workbench`'s `createPlayhead`) once per rendered
frame.

> **Incubating in etendue** (`web/packages/three-react`, `private`); moves to lab-ui with
> `@vitavision/three` (L8-1).

```tsx
<SceneCanvas className="h-full">
  <FrameTree baked={baked} playhead={playhead}>
    <Robot id="ur5e" visuals={manifest.visuals} resolve={meshUrl} />
    <AtFrame name="cam_left">
      <CameraFrustum borderRays={rays} depth={0.12} active onSelect={select} />
    </AtFrame>
    <AtFrame name="board">
      <TargetBoard width={0.25} height={0.175} checker={{ cols: 10, rows: 7 }} />
    </AtFrame>
  </FrameTree>
</SceneCanvas>
```

- `SceneCanvas` — Z-up canvas in vitavision colours: orbit controls, ground grid, lights.
- `FrameTree` / `AtFrame` / `useFrameTree` — a baked scenario as a frame graph; children of
  `AtFrame` live in that frame.
- `Robot`, `CameraFrustum`, `LaserFan`, `TargetBoard`, `LightGizmo`, `FrameAxes`.
- `useSceneColors` — the theme's scene colours, following the `dark` class.

Colours are tokens only: `signal` for selection, `fg-muted` for robots and idle cameras,
`defect` for laser light and X axes, `normal` Y, `signal` Z, `warn` lights.

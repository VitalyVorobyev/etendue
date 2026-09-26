#!/usr/bin/env python3
"""Build etendue robot assets from pinned upstream robot descriptions.

For every robot in ``robots.toml`` this script:

1. fetches the upstream repository at the pinned commit (shallow, cached, and
   verified: the checked-out HEAD must equal the pinned SHA);
2. expands the xacro entry with the pinned args into
   ``assets/robots/<id>/robot.urdf`` (xacro output verbatim, apart from a
   reproducible provenance banner replacing xacro's absolute-path banner);
3. converts every link's visual meshes into one ``meshes/<link>.glb`` in the
   LINK frame, with the URDF ``<visual><origin>`` and mesh ``scale`` baked
   into the vertices;
4. writes ``robot.json`` (the manifest parsed by ``etendue-scene``) and copies
   the upstream licence text next to it;
5. runs gate G1.3: every GLB is reloaded from disk and its vertex positions
   are compared, face corner by face corner, with the source mesh transformed
   through an independently written code path (tolerance 1e-6 m).

Mesh interpretation follows RViz (the reference consumer of ROS descriptions):

* COLLADA node transforms are applied; ``<up_axis>`` is ignored (RViz skips
  Assimp's up-axis root node); ``<unit meter="...">`` IS applied (RViz
  ``getMeshUnitRescale``). trimesh does not apply the unit, so we do.
* Materials embedded in the mesh are kept; the URDF ``<material><color>`` is
  used only for geometry that carries no material of its own.
* GLB positions are the raw link-frame coordinates (URDF convention: metres,
  +Z up in the link frame). No glTF Y-up conversion is applied.

Exactly-duplicate vertices (same position, normal and UV in float64) are
welded to keep the GLBs small; this is lossless and the gate compares face
corners, so it is covered by the round-trip check.

Usage (from the repository root)::

    uv run --locked --project tools/robot-assets tools/robot-assets/build.py \\
        --report docs/measurements/g1_3_robot_assets.md
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import importlib.metadata
import json
import math
import platform
import re
import shutil
import subprocess
import sys
import tomllib
import types
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import trimesh
import xacro
import xacro.substitution_args
import yaml
from lxml import etree

TOOL_DIR = Path(__file__).resolve().parent
REPO_ROOT = TOOL_DIR.parents[1]
ASSETS_DIR = REPO_ROOT / "assets" / "robots"
DEFAULT_CACHE = TOOL_DIR / ".cache"
GATE_TOLERANCE_M = 1e-6
MOVABLE = ("revolute", "continuous", "prismatic")
ID_RE = re.compile(r"^[A-Za-z0-9_-]+$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


class BuildError(RuntimeError):
    pass


# --------------------------------------------------------------------------- #
# Upstream sources
# --------------------------------------------------------------------------- #


def _git(args: list[str], cwd: Path) -> str:
    out = subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True)
    return out.stdout.strip()


def fetch_repo(url: str, revision: str, cache_dir: Path) -> Path:
    """Shallow-fetch ``url`` at commit ``revision`` into the cache and verify it."""
    if not SHA_RE.match(revision):
        raise BuildError(f"{url}: revision must be a 40-hex commit SHA, got {revision!r}")
    name = url.rstrip("/").removesuffix(".git").split("/")[-1]
    dest = cache_dir / f"{name}-{revision[:12]}"
    if not (dest / ".git").is_dir():
        partial = dest.with_name(dest.name + ".partial")
        shutil.rmtree(partial, ignore_errors=True)
        partial.mkdir(parents=True)
        print(f"  fetching {url} @ {revision[:12]}")
        _git(["init", "-q"], partial)
        _git(["remote", "add", "origin", url], partial)
        _git(["fetch", "-q", "--depth", "1", "origin", revision], partial)
        _git(["-c", "advice.detachedHead=false", "checkout", "-q", "--detach", "FETCH_HEAD"], partial)
        partial.rename(dest)
    head = _git(["rev-parse", "HEAD"], dest)
    if head != revision:
        raise BuildError(f"{dest}: HEAD is {head}, expected pinned {revision}")
    if _git(["status", "--porcelain"], dest):
        raise BuildError(f"{dest}: cached checkout is modified; delete it and rebuild")
    return dest


# --------------------------------------------------------------------------- #
# xacro -> URDF
# --------------------------------------------------------------------------- #


def expand_xacro(entry: Path, args: dict[str, str], packages: dict[str, Path], banner: list[str]) -> str:
    def find(pkg: str) -> str:
        if pkg not in packages:
            raise BuildError(f"$(find {pkg}): package not declared in robots.toml `packages`")
        return str(packages[pkg])

    # PyPI xacro resolves $(find) through ament_index; route it to the pinned checkout.
    xacro.substitution_args._eval_find = find
    doc = xacro.process_file(str(entry), mappings={k: str(v) for k, v in args.items()})
    # xacro prepends a banner containing the absolute input path; replace it
    # with a machine-independent one so the output is reproducible.
    for node in list(doc.childNodes):
        if node.nodeType == node.COMMENT_NODE:
            doc.removeChild(node)
    for line in banner:
        doc.insertBefore(doc.createComment(f" {line} "), doc.documentElement)
    return doc.toprettyxml(indent="  ")


# --------------------------------------------------------------------------- #
# URDF model
# --------------------------------------------------------------------------- #


def _floats(text: str | None, default: list[float]) -> list[float]:
    return [float(v) for v in text.split()] if text else list(default)


@dataclass
class Joint:
    name: str
    kind: str
    parent: str
    child: str
    limit: etree._Element | None
    mimic: bool


@dataclass
class Visual:
    xyz: list[float]
    rpy: list[float]
    mesh_uri: str
    scale: list[float]
    rgba: list[float] | None


@dataclass
class Urdf:
    root_link: str
    links: list[str]
    joints: list[Joint]
    visuals: dict[str, list[Visual]]

    def _to_root(self, link: str) -> list[Joint]:
        """Joints from ``link`` up to the root, child-most first."""
        by_child = {j.child: j for j in self.joints}
        out = []
        while link in by_child:
            out.append(by_child[link])
            link = by_child[link].parent
        return out

    def chain(self, base: str, tip: str) -> list[Joint]:
        """Joints on the path base -> tip, in base->tip order.

        ``base`` need not be an ancestor of ``tip`` (REP-199 ``base`` is a
        sibling branch of the arm), but the path from ``base`` up to the
        common ancestor must consist of fixed joints only. The returned list is
        the ancestor -> tip part, which carries every movable joint.
        """
        base_up, tip_up = self._to_root(base), self._to_root(tip)
        tip_ancestors = {tip} | {j.parent for j in tip_up}
        ancestor, base_side = base, []
        for joint in base_up:
            if ancestor in tip_ancestors:
                break
            base_side.append(joint)
            ancestor = joint.parent
        if ancestor not in tip_ancestors:
            raise BuildError(f"links {base!r} and {tip!r} share no common ancestor")
        moving = [j.name for j in base_side if j.kind != "fixed"]
        if moving:
            raise BuildError(f"path {base!r} -> common ancestor {ancestor!r} has non-fixed joints {moving}")
        tip_side = []
        for joint in tip_up:
            if joint.child == ancestor:
                break
            tip_side.append(joint)
        return tip_side[::-1]


def parse_urdf(text: str) -> Urdf:
    robot = etree.fromstring(text.encode())
    links = [link.get("name") for link in robot.findall("link")]
    if len(set(links)) != len(links):
        raise BuildError("duplicate link names in URDF")
    named_colors = {}
    for mat in robot.findall("material"):
        color = mat.find("color")
        if color is not None:
            named_colors[mat.get("name")] = _floats(color.get("rgba"), [])

    joints = []
    for j in robot.findall("joint"):
        joints.append(
            Joint(
                name=j.get("name"),
                kind=j.get("type"),
                parent=j.find("parent").get("link"),
                child=j.find("child").get("link"),
                limit=j.find("limit"),
                mimic=j.find("mimic") is not None,
            )
        )
    children = [j.child for j in joints]
    if len(set(children)) != len(children):
        raise BuildError("a link has more than one parent joint")
    roots = [name for name in links if name not in set(children)]
    if len(roots) != 1:
        raise BuildError(f"URDF must have exactly one root link, found {roots}")

    visuals: dict[str, list[Visual]] = {}
    for link in robot.findall("link"):
        for vis in link.findall("visual"):
            origin = vis.find("origin")
            geometry = vis.find("geometry")
            mesh = geometry.find("mesh") if geometry is not None else None
            if mesh is None:
                kinds = [c.tag for c in geometry] if geometry is not None else []
                raise BuildError(f"link {link.get('name')}: unsupported visual geometry {kinds}")
            rgba = None
            mat = vis.find("material")
            if mat is not None:
                color = mat.find("color")
                if color is not None:
                    rgba = _floats(color.get("rgba"), [])
                else:
                    rgba = named_colors.get(mat.get("name"))
            visuals.setdefault(link.get("name"), []).append(
                Visual(
                    xyz=_floats(origin.get("xyz") if origin is not None else None, [0, 0, 0]),
                    rpy=_floats(origin.get("rpy") if origin is not None else None, [0, 0, 0]),
                    mesh_uri=mesh.get("filename"),
                    scale=_floats(mesh.get("scale"), [1, 1, 1]),
                    rgba=rgba,
                )
            )
    return Urdf(root_link=roots[0], links=links, joints=joints, visuals=visuals)


def resolve_mesh_uri(uri: str, packages: dict[str, Path], checkout: Path) -> Path:
    if uri.startswith("package://"):
        pkg, _, rel = uri[len("package://") :].partition("/")
        if pkg not in packages:
            raise BuildError(f"{uri}: package {pkg!r} not declared in robots.toml `packages`")
        path = packages[pkg] / rel
    elif uri.startswith("file://"):
        path = Path(uri[len("file://") :])
    else:
        raise BuildError(f"{uri}: only package:// and file:// mesh URIs are supported")
    path = path.resolve()
    if checkout.resolve() not in path.parents:
        raise BuildError(f"{uri}: resolves outside the pinned checkout ({path})")
    if not path.is_file():
        raise BuildError(f"{uri}: file not found ({path})")
    return path


# --------------------------------------------------------------------------- #
# Meshes
# --------------------------------------------------------------------------- #


def collada_unit_meter(path: Path) -> float:
    """``<asset><unit meter>`` of a COLLADA file (1.0 if absent), read with lxml."""
    tree = etree.parse(str(path))
    unit = tree.find("{*}asset/{*}unit")
    return float(unit.get("meter", "1")) if unit is not None else 1.0


def source_unit_meter(path: Path) -> float:
    return collada_unit_meter(path) if path.suffix.lower() == ".dae" else 1.0


@contextlib.contextmanager
def collada_float64():
    """Make pycollada parse ``<float_array>`` data and node transforms as float64.

    pycollada hard-codes ``numpy.float32`` in ``collada.source`` and
    ``collada.scene``; that would quantise the source mesh before we ever
    transform it (and hide the quantisation from the G1.3 comparison). Both
    modules only use the name for parse/identity dtypes, so pointing their
    ``numpy`` at a shim whose ``float32`` is ``float64`` gives exact decimal
    parsing without touching anything else.
    """
    import collada.scene
    import collada.source

    shim = types.ModuleType("numpy_float64_for_pycollada")
    shim.__dict__.update(np.__dict__)
    shim.float32 = np.float64
    saved = (collada.source.numpy, collada.scene.numpy)
    collada.source.numpy = collada.scene.numpy = shim
    try:
        yield
    finally:
        collada.source.numpy, collada.scene.numpy = saved


def load_source_scene(path: Path) -> trimesh.Scene:
    """Load a source mesh as a Scene with NO trimesh processing.

    ``trimesh.load(..., process=False)`` does not reach the sub-meshes of the
    COLLADA loader (its scene geometries are built with ``process=True``,
    i.e. a rounding-based vertex merge at 1e-8). Build the scene from
    ``load_collada``'s raw output instead so every face corner is exactly
    the file's value.
    """
    if path.suffix.lower() != ".dae":
        mesh = trimesh.load_mesh(str(path), process=False)
        scene = trimesh.Scene()
        scene.add_geometry(mesh, geom_name=path.name, node_name=path.name)
        return scene
    from trimesh.exchange.dae import load_collada

    with collada_float64(), path.open("rb") as fh:
        loaded = load_collada(fh, resolver=trimesh.resolvers.FilePathResolver(str(path)))
    scene = trimesh.Scene()
    for name, kwargs in loaded["geometry"].items():
        scene.geometry[name] = trimesh.Trimesh(**kwargs, process=False, validate=False)
    for edge in loaded["graph"]:
        scene.graph.update(**edge)
    return scene


def source_geometries(path: Path) -> list[tuple[str, np.ndarray, trimesh.Trimesh]]:
    """(node name, node transform, geometry) for every geometry node, sorted by node."""
    scene = load_source_scene(path)
    items = []
    for node in scene.graph.nodes_geometry:
        transform, geom_name = scene.graph[node]
        items.append((str(node), np.asarray(transform, dtype=np.float64), scene.geometry[geom_name]))
    return sorted(items, key=lambda item: item[0])


def visual_matrix(xyz: list[float], rpy: list[float], scale: list[float]) -> np.ndarray:
    """link_T_mesh for a URDF visual: Trans(xyz) * Rz(y) Ry(p) Rx(r) * Scale."""
    m = trimesh.transformations.euler_matrix(rpy[0], rpy[1], rpy[2], "sxyz")
    m[:3, 3] = xyz
    return m @ np.diag([*scale, 1.0])


def _unitize(v: np.ndarray) -> np.ndarray:
    norm = np.linalg.norm(v, axis=1, keepdims=True)
    return np.divide(v, norm, out=np.zeros_like(v), where=norm > 0)


def weld_exact(columns: list[np.ndarray], faces: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Merge vertices whose attribute rows are exactly equal; keep first-occurrence order."""
    key = np.ascontiguousarray(np.hstack(columns))
    _, first, inverse = np.unique(key, axis=0, return_index=True, return_inverse=True)
    order = np.argsort(first, kind="stable")
    rank = np.empty_like(order)
    rank[order] = np.arange(len(order))
    return first[order], rank[inverse.reshape(-1)][faces]


@dataclass
class Part:
    """One source geometry of one visual, baked into the link frame."""

    name: str  # node and geometry name inside the GLB
    source: Path
    source_node: str
    visual: Visual
    mesh: trimesh.Trimesh


@dataclass
class LinkMesh:
    link: str
    glb: Path
    parts: list[Part] = field(default_factory=list)


def bake_part(name: str, source: Path, node: str, node_tf: np.ndarray, geom: trimesh.Trimesh, vis: Visual, unit: float) -> Part:
    matrix = visual_matrix(vis.xyz, vis.rpy, vis.scale) @ np.diag([unit, unit, unit, 1.0]) @ node_tf
    linear, translation = matrix[:3, :3], matrix[:3, 3]

    vertices = np.asarray(geom.vertices, dtype=np.float64) @ linear.T + translation
    faces = np.asarray(geom.faces, dtype=np.int64)
    if np.linalg.det(linear) < 0:
        faces = faces[:, ::-1]
    columns = [vertices]

    normals = None
    if "vertex_normals" in geom._cache.cache:
        normals = _unitize(np.asarray(geom.vertex_normals, dtype=np.float64) @ np.linalg.inv(linear))
        columns.append(normals)

    uv, material = None, None
    visual = geom.visual
    if isinstance(visual, trimesh.visual.TextureVisuals):
        material = visual.material.copy() if visual.material is not None else None
        if visual.uv is not None:
            uv = np.asarray(visual.uv, dtype=np.float64)
            columns.append(uv)
    elif isinstance(visual, trimesh.visual.ColorVisuals):
        if visual.kind is not None:
            raise BuildError(f"{source}:{node}: per-vertex/face colours are not supported")
    if material is None and vis.rgba is not None:
        rgba = np.clip(np.round(np.asarray(vis.rgba) * 255.0), 0, 255).astype(np.uint8)
        material = trimesh.visual.material.PBRMaterial(name="urdf_material", baseColorFactor=rgba)

    keep, faces = weld_exact(columns, faces)
    mesh = trimesh.Trimesh(
        vertices=vertices[keep],
        faces=faces,
        vertex_normals=normals[keep] if normals is not None else None,
        visual=trimesh.visual.TextureVisuals(uv=uv[keep] if uv is not None else None, material=material)
        if material is not None
        else None,
        process=False,
        validate=False,
    )
    return Part(name=name, source=source, source_node=node, visual=vis, mesh=mesh)


def build_link_mesh(link: str, visuals: list[Visual], packages: dict[str, Path], checkout: Path, out_dir: Path) -> LinkMesh:
    result = LinkMesh(link=link, glb=out_dir / f"{link}.glb")
    scene = trimesh.Scene()
    for vi, vis in enumerate(visuals):
        source = resolve_mesh_uri(vis.mesh_uri, packages, checkout)
        unit = source_unit_meter(source)
        for gi, (node, node_tf, geom) in enumerate(source_geometries(source)):
            part = bake_part(f"{link}.v{vi}.g{gi}", source, node, node_tf, geom, vis, unit)
            result.parts.append(part)
            scene.add_geometry(part.mesh, geom_name=part.name, node_name=part.name)
    result.glb.write_bytes(scene.export(file_type="glb"))
    return result


# --------------------------------------------------------------------------- #
# Gate G1.3 (independent code path: explicit rotation matrices, pycollada unit)
# --------------------------------------------------------------------------- #


def _rpy_matrix(roll: float, pitch: float, yaw: float) -> np.ndarray:
    cr, sr, cp, sp, cy, sy = map(float, (math.cos(roll), math.sin(roll), math.cos(pitch), math.sin(pitch), math.cos(yaw), math.sin(yaw)))
    rx = np.array([[1, 0, 0], [0, cr, -sr], [0, sr, cr]])
    ry = np.array([[cp, 0, sp], [0, 1, 0], [-sp, 0, cp]])
    rz = np.array([[cy, -sy, 0], [sy, cy, 0], [0, 0, 1]])
    return rz @ ry @ rx


def _pycollada_unit(path: Path) -> float:
    if path.suffix.lower() != ".dae":
        return 1.0
    import collada

    unit = collada.Collada(str(path), ignore=[collada.common.DaeError]).assetInfo.unitmeter
    return 1.0 if unit is None else float(unit)


def _collada_position_rows(path: Path) -> np.ndarray:
    """All POSITION rows of a COLLADA file, parsed from the XML text as float64 (lxml)."""
    tree = etree.parse(str(path))
    sources = {s.get("id"): s for s in tree.iter("{*}source")}
    rows = []
    for vertices in tree.iter("{*}vertices"):
        for inp in vertices.findall("{*}input"):
            if inp.get("semantic") == "POSITION":
                array = sources[inp.get("source").lstrip("#")].find("{*}float_array")
                rows.append(np.array(array.text.split(), dtype=np.float64).reshape(-1, 3))
    return np.unique(np.vstack(rows), axis=0)


def _rows_subset(rows: np.ndarray, reference: np.ndarray) -> bool:
    view = np.dtype([("x", "<f8"), ("y", "<f8"), ("z", "<f8")])
    a = np.ascontiguousarray(rows, dtype="<f8").view(view).ravel()
    b = np.ascontiguousarray(reference, dtype="<f8").view(view).ravel()
    return bool(np.isin(a, b).all())


@dataclass
class GateRow:
    link: str
    source: str
    source_vertices: int
    glb_vertices: int
    max_error_m: float


def gate_check(link_meshes: list[LinkMesh], checkout: Path) -> list[GateRow]:
    rows = []
    for lm in link_meshes:
        loaded = trimesh.load(str(lm.glb), force="scene", process=False)
        per_source: dict[Path, GateRow] = {}
        sources: dict[Path, dict[str, tuple[np.ndarray, trimesh.Trimesh]]] = {}
        units: dict[Path, float] = {}
        for part in lm.parts:
            if part.source not in sources:
                fresh = load_source_scene(part.source)
                vertex_dtypes = {g.vertices.dtype for g in fresh.geometry.values()}
                if vertex_dtypes != {np.dtype(np.float64)}:
                    raise BuildError(f"{part.source}: source parsed as {vertex_dtypes}, expected float64")
                if part.source.suffix.lower() == ".dae":
                    text_rows = _collada_position_rows(part.source)
                    for g in fresh.geometry.values():
                        if len(g.vertices) != 3 * len(g.faces):
                            raise BuildError(f"{part.source}: source geometry was processed (vertices merged)")
                        if not _rows_subset(np.asarray(g.vertices), text_rows):
                            raise BuildError(f"{part.source}: loaded vertices are not exact rows of the file's POSITION arrays")
                sources[part.source] = {str(n): (np.asarray(fresh.graph[n][0]), fresh.geometry[fresh.graph[n][1]]) for n in fresh.graph.nodes_geometry}
                units[part.source] = _pycollada_unit(part.source)
            node_tf, src = sources[part.source][part.source_node]

            vis = part.visual
            unit = units[part.source]
            linear = _rpy_matrix(*vis.rpy) @ np.diag(vis.scale) @ (unit * node_tf[:3, :3])
            offset = _rpy_matrix(*vis.rpy) @ np.diag(vis.scale) @ (unit * node_tf[:3, 3]) + np.asarray(vis.xyz)
            src_faces = np.asarray(src.faces)
            if np.linalg.det(linear) < 0:
                src_faces = src_faces[:, ::-1]
            expected = (np.asarray(src.vertices, dtype=np.float64) @ linear.T + offset)[src_faces]

            glb_tf, glb_geom_name = loaded.graph[part.name]
            glb_geom = loaded.geometry[glb_geom_name]
            glb_vertices = np.asarray(glb_geom.vertices, dtype=np.float64) @ np.asarray(glb_tf)[:3, :3].T + np.asarray(glb_tf)[:3, 3]
            actual = glb_vertices[np.asarray(glb_geom.faces)]
            if actual.shape != expected.shape:
                raise BuildError(f"{lm.glb.name}:{part.name}: {actual.shape} face corners vs source {expected.shape}")
            err = float(np.max(np.abs(actual - expected))) if expected.size else 0.0

            row = per_source.setdefault(
                part.source,
                GateRow(lm.link, str(part.source.relative_to(checkout)), 0, 0, 0.0),
            )
            row.source_vertices += len(src.vertices)
            row.glb_vertices += len(glb_geom.vertices)
            row.max_error_m = max(row.max_error_m, err)
        rows.extend(per_source.values())
    return rows


# --------------------------------------------------------------------------- #
# Manifest
# --------------------------------------------------------------------------- #


def acceleration_limits(spec: dict, joint_names: list[str], cache_dir: Path) -> dict[str, tuple[float, str]]:
    kind = spec["kind"]
    if kind == "default":
        value = float(spec["value"])
        return {name: (value, f"default: {spec['reason']}") for name in joint_names}
    if kind == "yaml":
        checkout = fetch_repo(spec["repository"], spec["revision"], cache_dir)
        data = yaml.safe_load((checkout / spec["file"]).read_text())
        out = {}
        for name in joint_names:
            key = spec["key"].replace("<joint>", name)
            node = data
            for part in key.split("."):
                node = node[part]
            value = float(node)
            if not value > 0:
                raise BuildError(f"{key}: acceleration limit must be > 0, got {value}")
            url = f"{spec['repository']}/blob/{spec['revision']}/{spec['file']}"
            out[name] = (value, f"{url} key {key}")
        return out
    raise BuildError(f"unknown acceleration kind {kind!r}")


def joint_entries(chain: list[Joint], accel: dict[str, tuple[float, str]]) -> list[dict]:
    entries = []
    for j in chain:
        if j.kind not in MOVABLE:
            continue
        if j.mimic:
            raise BuildError(f"joint {j.name}: mimic joints are not supported")
        if j.limit is None or j.limit.get("velocity") is None:
            raise BuildError(f"joint {j.name}: missing <limit velocity>")
        velocity = float(j.limit.get("velocity"))
        if j.kind == "continuous":
            lower, upper = -math.pi, math.pi
            position_src = "urdf velocity; position: continuous joint without limits, [-pi, pi] by convention"
        else:
            lower, upper = float(j.limit.get("lower")), float(j.limit.get("upper"))
            position_src = "urdf position/velocity"
        if not lower < upper or not velocity > 0:
            raise BuildError(f"joint {j.name}: invalid limits lower={lower} upper={upper} velocity={velocity}")
        max_acc, acc_src = accel[j.name]
        entries.append(
            {
                "name": j.name,
                "lower": lower,
                "upper": upper,
                "max_velocity": velocity,
                "max_acceleration": max_acc,
                "limit_source": f"{position_src}; acceleration: {acc_src}",
            }
        )
    return entries


@dataclass
class RobotResult:
    id: str
    name: str
    revision: str
    repository: str
    root_link: str
    base_link: str
    tcp_link: str
    rows: list[GateRow]


def build_robot(cfg: dict, cache_dir: Path) -> RobotResult:
    rid = cfg["id"]
    if not ID_RE.match(rid) or rid == "world":
        raise BuildError(f"invalid robot id {rid!r}")
    print(f"[{rid}]")
    checkout = fetch_repo(cfg["repository"], cfg["revision"], cache_dir)
    packages = {pkg: (checkout / rel).resolve() for pkg, rel in cfg["packages"].items()}
    xacro_args = dict(cfg.get("xacro_args", {}))

    out_dir = ASSETS_DIR / rid
    mesh_dir = out_dir / "meshes"
    shutil.rmtree(mesh_dir, ignore_errors=True)
    mesh_dir.mkdir(parents=True)

    args_text = " ".join(f"{k}:={v}" for k, v in xacro_args.items()) or "(none)"
    banner = [
        f"Expanded by etendue tools/robot-assets/build.py with xacro {importlib.metadata.version('xacro')}",
        f"source: {cfg['repository']} @ {cfg['revision']}",
        f"entry: {cfg['entry']}   xacro args: {args_text}",
        "Do not edit: regenerate with build.py.",
    ]
    urdf_text = expand_xacro(checkout / cfg["entry"], xacro_args, packages, banner)
    (out_dir / "robot.urdf").write_text(urdf_text)
    urdf = parse_urdf(urdf_text)
    for link in (cfg["base_link"], cfg["tcp_link"]):
        if link not in urdf.links:
            raise BuildError(f"{link!r} is not a link of the expanded URDF")

    chain = urdf.chain(cfg["base_link"], cfg["tcp_link"])
    for j in chain:
        if j.kind not in (*MOVABLE, "fixed"):
            raise BuildError(f"joint {j.name}: unsupported type {j.kind!r} on the base->tcp chain")
    movable_names = [j.name for j in chain if j.kind in MOVABLE]
    accel = acceleration_limits(cfg["acceleration"], movable_names, cache_dir)
    joints = joint_entries(chain, accel)

    link_meshes = []
    for link in urdf.links:
        if link in urdf.visuals:
            link_meshes.append(build_link_mesh(link, urdf.visuals[link], packages, checkout, mesh_dir))
            print(f"  {link}.glb  ({sum(len(p.mesh.vertices) for p in link_meshes[-1].parts)} vertices)")

    lic = cfg["license"]
    notes = lic["notes"].strip()
    if cfg.get("opw_preset"):
        notes += (
            f" OPW preset: rs-opw-kinematics 3.0.0 Parameters::{cfg['opw_preset']}(), which maps "
            f"{cfg['base_link']} -> {cfg['tcp_link']} in these URDF joint coordinates (the crate's "
            "robot_presets test checks it against this URDF at the same upstream SHA)."
        )
    manifest = {
        "version": 1,
        "id": rid,
        "name": cfg["name"],
        "urdf": "robot.urdf",
        "base_link": cfg["base_link"],
        "tcp_link": cfg["tcp_link"],
        "joints": joints,
        "visuals": [{"link": lm.link, "mesh": f"meshes/{lm.glb.name}"} for lm in link_meshes],
        "source": {
            "repository": cfg["repository"],
            "revision": cfg["revision"],
            "entry": cfg["entry"],
            "xacro_args": xacro_args,
        },
        "license": {
            "urdf": lic["urdf"],
            "meshes": lic["meshes"],
            "meshes_redistributable": bool(lic["meshes_redistributable"]),
            "notes": notes,
        },
    }
    (out_dir / "robot.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    license_text = "\n".join((checkout / f).read_text() for f in cfg["license_files"])
    (out_dir / "LICENSE").write_text(license_text)

    rows = gate_check(link_meshes, checkout)
    worst = max((r.max_error_m for r in rows), default=0.0)
    print(f"  root link: {urdf.root_link}; joints: {movable_names}; G1.3 max error {worst:.3e} m")
    return RobotResult(rid, cfg["name"], cfg["revision"], cfg["repository"], urdf.root_link, cfg["base_link"], cfg["tcp_link"], rows)


# --------------------------------------------------------------------------- #
# Report
# --------------------------------------------------------------------------- #


def write_report(path: Path, results: list[RobotResult], command: str) -> bool:
    worst = max((r.max_error_m for res in results for r in res.rows), default=0.0)
    passed = worst <= GATE_TOLERANCE_M
    etendue = _git(["describe", "--always", "--dirty"], REPO_ROOT)
    versions = {
        "python": platform.python_version(),
        **{pkg: importlib.metadata.version(pkg) for pkg in ("trimesh", "pycollada", "numpy", "lxml", "xacro", "pyyaml")},
        "git": _git(["--version"], REPO_ROOT).removeprefix("git version "),
    }
    lines = [
        "# G1.3 — Robot asset mesh round-trip",
        "",
        f"- Date: {dt.date.today().isoformat()}",
        f"- etendue commit: `{etendue}` (`git describe --always --dirty` at measurement time)",
        f"- Result: **{'PASS' if passed else 'FAIL'}** — overall max abs vertex error {worst:.3e} m "
        f"(gate ≤ {GATE_TOLERANCE_M:.0e} m)",
        "",
        "## Criterion",
        "",
        "Ticket P1-2 (docs/pivot/PLAN.md §5): per-mesh vertex positions round-trip within ≤ 1e-6 m of",
        "the source mesh. For every link GLB written by `tools/robot-assets/build.py`, the GLB is",
        "reloaded from disk with trimesh (`process=False`) and each face corner's position (GLB node",
        "transform applied) is compared with the same corner of the source mesh, freshly reloaded and",
        "transformed by the URDF `<visual><origin>`, mesh `scale` and COLLADA node transform / `<unit>`.",
        "The expected positions use a separate code path from the builder (explicit Rz·Ry·Rx matrices,",
        "unit read with pycollada instead of lxml). Face counts and corner order must match exactly;",
        "the value reported is the max absolute coordinate difference in metres.",
        "",
        "Source precision: pycollada hard-codes float32 when parsing `<float_array>` and node",
        "transforms, which would quantise the source before the comparison and hide that error. Both",
        "the builder and the gate therefore parse COLLADA in float64 (see `collada_float64` in",
        "`build.py`), and the gate additionally asserts that every loaded source vertex is an exact",
        "row of a POSITION `<float_array>` parsed from the XML text with lxml. The only remaining",
        "error is the single float32 rounding of glTF positions (half-ULP ≈ 3e-8 m at 0.5 m).",
        "",
        "## Pinned sources",
        "",
        "| Robot | Repository | Revision | URDF root | base_link -> tcp_link |",
        "|---|---|---|---|---|",
    ]
    for res in results:
        lines.append(
            f"| `{res.id}` ({res.name}) | {res.repository} | `{res.revision}` | `{res.root_link}` "
            f"| `{res.base_link}` -> `{res.tcp_link}` |"
        )
    lines += [
        "",
        "Base frame: robot.json `base_link` is the REP-199 `base` link, the frame the controller",
        "reports TCP poses in (ADR 0002). For UR5e it is `base_link`·Rz(π); for the ABB arm it is",
        "identical to `base_link`. It is a fixed-joint sibling of the arm, not an ancestor of the TCP.",
        "The GLBs are in each link's own frame, so this choice does not affect G1.3.",
        "",
        "The meshes are git-ignored (licence permits redistribution, but they are regenerated",
        "byte-identically from the pinned SHAs instead of being committed).",
        "",
        "## Per-mesh results",
        "",
    ]
    for res in results:
        robot_worst = max((r.max_error_m for r in res.rows), default=0.0)
        lines += [
            f"### `{res.id}` — max {robot_worst:.3e} m (URDF root link: `{res.root_link}`)",
            "",
            "| Link | Source mesh (repo-relative) | Source vertices | GLB vertices (welded) | Max abs error (m) |",
            "|---|---|---:|---:|---:|",
        ]
        for r in res.rows:
            lines.append(f"| `{r.link}` | `{r.source}` | {r.source_vertices} | {r.glb_vertices} | {r.max_error_m:.3e} |")
        lines.append("")
    lines += [
        "Source vertices are counted as trimesh loads them (`process=False`; COLLADA triangles are",
        "unshared, three vertices per face). GLB vertices are after exact welding of identical",
        "position+normal+UV rows.",
        "",
        "## Tool versions",
        "",
        *[f"- {k}: {v}" for k, v in versions.items()],
        "",
        "## Reproduce",
        "",
        "```bash",
        command,
        "```",
        "",
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines))
    return passed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--config", type=Path, default=TOOL_DIR / "robots.toml")
    parser.add_argument("--robot", action="append", help="build only this robot id (repeatable)")
    parser.add_argument("--cache-dir", type=Path, default=DEFAULT_CACHE, help="upstream checkout cache")
    parser.add_argument("--report", type=Path, help="write the G1.3 measurement report (Markdown) here")
    opts = parser.parse_args()

    robots = tomllib.loads(opts.config.read_text())["robot"]
    ids = [r["id"] for r in robots]
    if len(set(ids)) != len(ids):
        raise BuildError("duplicate robot ids in robots.toml")
    if opts.robot:
        unknown = set(opts.robot) - set(ids)
        if unknown:
            raise BuildError(f"unknown robot ids: {sorted(unknown)}")
        robots = [r for r in robots if r["id"] in opts.robot]

    results = [build_robot(cfg, opts.cache_dir.resolve()) for cfg in robots]
    worst = max((r.max_error_m for res in results for r in res.rows), default=0.0)
    passed = worst <= GATE_TOLERANCE_M
    if opts.report:
        command = "uv run --locked --project tools/robot-assets tools/robot-assets/build.py --report " + str(
            opts.report.resolve().relative_to(REPO_ROOT)
        )
        write_report(opts.report.resolve(), results, command)
        print(f"report: {opts.report}")
    print(f"G1.3 {'PASS' if passed else 'FAIL'}: max abs vertex error {worst:.3e} m (gate {GATE_TOLERANCE_M:.0e} m)")
    return 0 if passed else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except BuildError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(2)

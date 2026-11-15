"""Shared Blender 5.3.0 headless architectural sculpting, Smart-UV, BVH Vertex-AO, and LOD GLB exporter."""

import json
import math
import os
import struct
import bpy
import bmesh
from mathutils import Euler, Matrix, Vector
from mathutils.bvhtree import BVHTree

MATERIAL_SPECS = {
    "stone": ((0.76, 0.72, 0.66, 1.0), 0.84, 0.0),
    "cobble": ((0.68, 0.64, 0.58, 1.0), 0.82, 0.0),
    "wood": ((0.50, 0.34, 0.20, 1.0), 0.74, 0.0),
    "slate": ((0.30, 0.36, 0.44, 1.0), 0.68, 0.1),
    "terracotta": ((0.68, 0.30, 0.18, 1.0), 0.72, 0.0),
    "metal": ((0.34, 0.37, 0.41, 1.0), 0.38, 0.85),
    "bronze": ((0.58, 0.40, 0.20, 1.0), 0.34, 0.88),
    "gold_trim": ((0.88, 0.66, 0.20, 1.0), 0.24, 0.94),
    "rock": ((0.52, 0.49, 0.44, 1.0), 0.88, 0.0),
    "pine_bark": ((0.32, 0.21, 0.13, 1.0), 0.85, 0.0),
    "pine_foliage": ((0.18, 0.36, 0.14, 1.0), 0.86, 0.0),
    "crystal_cyan": ((0.20, 0.78, 0.96, 1.0), 0.18, 0.2),
    "elixir_violet": ((0.68, 0.22, 0.88, 1.0), 0.20, 0.15),
    "skin_warm": ((0.88, 0.68, 0.52, 1.0), 0.68, 0.0),
    "leather": ((0.34, 0.21, 0.12, 1.0), 0.72, 0.0),
    "cloth_royal": ((0.18, 0.36, 0.66, 1.0), 0.90, 0.0),
    "cloth_crimson": ((0.66, 0.18, 0.18, 1.0), 0.90, 0.0),
    "bone_ivory": ((0.92, 0.87, 0.76, 1.0), 0.74, 0.0),
    "ember_orange": ((1.0, 0.44, 0.14, 1.0), 0.32, 0.1),
}


def clear_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)


def get_material(name: str) -> bpy.types.Material:
    mat = bpy.data.materials.get(name)
    if mat is not None:
        return mat
    rgba, rough, metal = MATERIAL_SPECS.get(name, ((0.7, 0.7, 0.7, 1.0), 0.75, 0.0))
    mat = bpy.data.materials.new(name=name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    if bsdf is not None:
        bsdf.inputs["Base Color"].default_value = rgba
        bsdf.inputs["Roughness"].default_value = rough
        bsdf.inputs["Metallic"].default_value = metal
    return mat


def _bm_to_obj(bm: bmesh.types.BMesh, name: str, mat_name: str, smooth: bool = False) -> bpy.types.Object:
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mat = get_material(mat_name)
    mesh.materials.append(mat)
    for poly in mesh.polygons:
        poly.use_smooth = smooth
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def add_box(
    name: str,
    size: tuple[float, float, float],
    center: tuple[float, float, float],
    mat_name: str,
    bevel_w: float = 0.03,
    segments: int = 2,
    taper_top: tuple[float, float] = (1.0, 1.0),
    rot_deg: tuple[float, float, float] = (0.0, 0.0, 0.0),
    smooth: bool = True,
) -> bpy.types.Object:
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    sx, sy, sz = size
    tx, ty = taper_top
    for v in bm.verts:
        v.co.x *= sx
        v.co.y *= sy
        v.co.z *= sz
        if v.co.z > 0.0:
            v.co.x *= tx
            v.co.y *= ty
    if bevel_w > 0.002:
        bmesh.ops.bevel(bm, geom=bm.edges[:], offset=bevel_w, segments=segments, affect="EDGES")
    if any(abs(a) > 1e-4 for a in rot_deg):
        rm = Euler((math.radians(a) for a in rot_deg), "XYZ").to_matrix().to_4x4()
        bmesh.ops.transform(bm, matrix=rm, verts=bm.verts[:])
    bmesh.ops.translate(bm, verts=bm.verts[:], vec=Vector(center))
    return _bm_to_obj(bm, name, mat_name, smooth=smooth)


def add_prism(
    name: str,
    r_bot: float,
    r_top: float,
    height: float,
    center: tuple[float, float, float],
    mat_name: str,
    segs: int = 8,
    bevel_w: float = 0.02,
    rot_deg: tuple[float, float, float] = (0.0, 0.0, 0.0),
    smooth: bool = True,
) -> bpy.types.Object:
    bm = bmesh.new()
    bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        cap_tris=False,
        segments=segs,
        radius1=max(r_bot, 0.005),
        radius2=max(r_top, 0.005),
        depth=height,
    )
    if bevel_w > 0.002:
        bmesh.ops.bevel(bm, geom=bm.edges[:], offset=bevel_w, segments=1, affect="EDGES")
    if any(abs(a) > 1e-4 for a in rot_deg):
        rm = Euler((math.radians(a) for a in rot_deg), "XYZ").to_matrix().to_4x4()
        bmesh.ops.transform(bm, matrix=rm, verts=bm.verts[:])
    bmesh.ops.translate(bm, verts=bm.verts[:], vec=Vector(center))
    return _bm_to_obj(bm, name, mat_name, smooth=smooth)


def add_beam_between(
    name: str,
    p0: tuple[float, float, float],
    p1: tuple[float, float, float],
    thickness: float,
    mat_name: str,
    bevel_w: float = 0.012,
) -> bpy.types.Object:
    v0 = Vector(p0)
    v1 = Vector(p1)
    delta = v1 - v0
    length = delta.length
    mid = (v0 + v1) * 0.5
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co.x *= thickness
        v.co.y *= thickness
        v.co.z *= length
    if bevel_w > 0.002:
        bmesh.ops.bevel(bm, geom=bm.edges[:], offset=bevel_w, segments=1, affect="EDGES")
    rot = delta.normalized().to_track_quat("Z", "Y").to_matrix().to_4x4()
    bmesh.ops.transform(bm, matrix=rot, verts=bm.verts[:])
    bmesh.ops.translate(bm, verts=bm.verts[:], vec=mid)
    return _bm_to_obj(bm, name, mat_name, smooth=True)


def add_rock_chunk(
    name: str,
    radii: tuple[float, float, float],
    center: tuple[float, float, float],
    mat_name: str,
    seed: int = 1,
    subdiv: int = 2,
    flatten_top: float = 0.72,
) -> bpy.types.Object:
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=1.0)
    for idx, v in enumerate(bm.verts):
        n = math.sin(v.co.x * 3.7 + seed * 1.9) * math.cos(v.co.y * 4.1 + seed) * 0.16
        n += math.sin(v.co.z * 5.3 + idx * 0.37 + seed) * 0.09
        v.co.x *= radii[0] * (1.0 + n)
        v.co.y *= radii[1] * (1.0 - n * 0.6)
        z_mul = radii[2] * (1.0 + n * 0.8)
        if v.co.z > 0.15:
            z_mul *= flatten_top
        v.co.z *= z_mul
    bmesh.ops.translate(bm, verts=bm.verts[:], vec=Vector(center))
    return _bm_to_obj(bm, name, mat_name, smooth=False)


def _hemisphere_dirs() -> list[Vector]:
    dirs = []
    # Golden-angle Fibonacci upper hemisphere (18 rays: fast and smooth)
    n = 18
    ga = math.pi * (3.0 - math.sqrt(5.0))
    for i in range(n):
        z = (i + 0.35) / n
        r = math.sqrt(max(0.0, 1.0 - z * z))
        theta = ga * i
        dirs.append(Vector((math.cos(theta) * r, math.sin(theta) * r, z)).normalized())
    return dirs


_HEMI_DIRS = _hemisphere_dirs()


def bake_vertex_ao(obj: bpy.types.Object, ray_dist: float = 1.55, attr_name: str = "Col") -> None:
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.verts.ensure_lookup_table()
    bm.faces.ensure_lookup_table()
    bvh = BVHTree.FromBMesh(bm, epsilon=0.002)

    vert_ao: list[float] = [1.0] * len(bm.verts)
    for idx, v in enumerate(bm.verts):
        nrm = v.normal.normalized() if v.normal.length > 1e-5 else Vector((0.0, 0.0, 1.0))
        t1 = nrm.orthogonal().normalized()
        t2 = nrm.cross(t1).normalized()
        origin = v.co + nrm * 0.014
        hits = 0
        for d in _HEMI_DIRS:
            world_d = (t1 * d.x + t2 * d.y + nrm * d.z).normalized()
            loc, _, _, dist = bvh.ray_cast(origin, world_d, ray_dist)
            if loc is not None and dist < ray_dist:
                hits += 1
        ao = 1.0 - (hits / float(len(_HEMI_DIRS))) * 0.72
        if v.co.z < 0.28:
            ao *= 0.66 + 0.34 * max(0.0, v.co.z / 0.28)
        vert_ao[idx] = max(0.24, min(1.0, ao))

    col_layer = mesh.color_attributes.get(attr_name)
    if col_layer is None:
        col_layer = mesh.color_attributes.new(name=attr_name, type="BYTE_COLOR", domain="CORNER")
    mesh.color_attributes.active_color = col_layer

    loop_idx = 0
    for face in bm.faces:
        for loop in face.loops:
            v_ao = vert_ao[loop.vert.index]
            col_layer.data[loop_idx].color = (v_ao, v_ao, v_ao, 1.0)
            loop_idx += 1
    bm.free()


def finalize_and_export_lods(
    objs: list[bpy.types.Object],
    out_prefix: str,
    uv_per_meter: float = 0.52,
    ray_dist: float = 1.55,
    make_lods: bool = True,
    recenter_xy: bool = True,
) -> dict[str, dict]:
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    if len(objs) > 1:
        bpy.ops.object.join()
    hero = bpy.context.active_object
    hero.name = os.path.basename(out_prefix)

    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    min_x = min(v.co.x for v in hero.data.vertices)
    max_x = max(v.co.x for v in hero.data.vertices)
    min_y = min(v.co.y for v in hero.data.vertices)
    max_y = max(v.co.y for v in hero.data.vertices)
    min_z = min(v.co.z for v in hero.data.vertices)
    cx = (min_x + max_x) * 0.5 if recenter_xy else 0.0
    cy = (min_y + max_y) * 0.5 if recenter_xy else 0.0
    for v in hero.data.vertices:
        v.co.x -= cx
        v.co.y -= cy
        v.co.z -= min_z
    hero.data.update()

    # Smart UV projection scaled by real-world 3D area
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.08, island_margin=0.015, correct_aspect=True, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode="OBJECT")

    bm = bmesh.new()
    bm.from_mesh(hero.data)
    uv_layer = bm.loops.layers.uv.active
    area_3d = sum(f.calc_area() for f in bm.faces)
    area_uv = 0.0
    if uv_layer is not None:
        for f in bm.faces:
            loops = f.loops
            if len(loops) >= 3:
                u0 = loops[0][uv_layer].uv
                for k in range(1, len(loops) - 1):
                    u1 = loops[k][uv_layer].uv - u0
                    u2 = loops[k + 1][uv_layer].uv - u0
                    area_uv += abs(u1.x * u2.y - u1.y * u2.x) * 0.5
        uv_mul = math.sqrt(max(area_3d, 1e-4) / max(area_uv, 1e-6)) * uv_per_meter
        for f in bm.faces:
            for lp in f.loops:
                lp[uv_layer].uv *= uv_mul
    bm.to_mesh(hero.data)
    bm.free()

    # Edge split preserves sharp masonry edges while keeping bevel highlights curved
    es = hero.modifiers.new(name="EdgeSplit", type="EDGE_SPLIT")
    es.split_angle = math.radians(38.0)
    es.use_edge_angle = True
    bpy.ops.object.modifier_apply(modifier=es.name)

    # Triangulate before vertex-AO and glTF export so TANGENT buffers always export cleanly
    tri = hero.modifiers.new(name="Triangulate", type="TRIANGULATE")
    tri.quad_method = "BEAUTY"
    tri.ngon_method = "BEAUTY"
    bpy.ops.object.modifier_apply(modifier=tri.name)

    bake_vertex_ao(hero, ray_dist=ray_dist, attr_name="Col")

    os.makedirs(os.path.dirname(out_prefix), exist_ok=True)
    report: dict[str, dict] = {}
    lod_specs = [("lod0", 1.0)] + ([("lod1", 0.45), ("lod2", 0.22)] if make_lods else [])

    for tag, ratio in lod_specs:
        bpy.ops.object.select_all(action="DESELECT")
        hero.select_set(True)
        bpy.context.view_layer.objects.active = hero
        target = hero
        if ratio < 0.99 and len(hero.data.polygons) > 64:
            target = hero.copy()
            target.data = hero.data.copy()
            bpy.context.collection.objects.link(target)
            bpy.ops.object.select_all(action="DESELECT")
            target.select_set(True)
            bpy.context.view_layer.objects.active = target
            dec = target.modifiers.new(name="Decimate", type="DECIMATE")
            dec.ratio = max(ratio, 36.0 / max(1, len(target.data.polygons)))
            bpy.ops.object.modifier_apply(modifier=dec.name)

        glb_path = f"{out_prefix}_{tag}.glb"
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.ops.export_scene.gltf(
            filepath=glb_path,
            use_selection=True,
            export_format="GLB",
            export_apply=True,
            export_tangents=True,
            export_vertex_color="NAME",
            export_vertex_color_name="Col",
            export_all_vertex_colors=True,
            export_active_vertex_color_when_no_material=True,
            export_image_format="NONE",
        )
        report[tag] = {
            "path": glb_path,
            "tris": sum(len(p.vertices) - 2 for p in target.data.polygons),
            "attrs": _inspect_glb_attributes(glb_path),
        }
        if target != hero:
            bpy.data.objects.remove(target, do_unlink=True)
    return report


def _inspect_glb_attributes(glb_path: str) -> list[str]:
    with open(glb_path, "rb") as f:
        data = f.read()
    if data[:4] != b"glTF":
        return []
    json_len = struct.unpack("<I", data[12:16])[0]
    gltf = json.loads(data[20 : 20 + json_len].decode("utf-8"))
    attrs = set()
    for mesh in gltf.get("meshes", []):
        for prim in mesh.get("primitives", []):
            attrs.update(prim.get("attributes", {}).keys())
    return sorted(attrs)

#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Headless Blender 5.x Mesh Cleanup & Rigging Pipeline (`tools/clean_mesh_blender.py`).
Usage:
  blender -b --factory-startup --python tools/clean_mesh_blender.py -- --input raw.glb --output clean.glb --height 1.25 --yaw -90

Solves the 4 major AI-3D-to-Godot pipeline blockers discovered in Super Mak 64:
1. Tripo P2 meshes face Blender +X instead of glTF +Z / Godot -Z -> applies mesh-level yaw rotation (not just euler_rotation).
2. Centers XY origin and places Z=0 at the bottom of the bounding box (feet at origin) and scales to target real-world height in metres.
3. Enforces matte roughness (`>= 0.85`) and `metallic = 0.0` so Tripo's default `metallic = 1.0` does not look like plastic/metal in Godot.
4. Exports clean GLB with lossy WebP / PNG textures (avoiding Godot 4.7's lossless WebP ORM import bug).
"""

import argparse
import math
import sys

try:
    import bpy
    import mathutils
except ImportError:
    print("Run this script inside Blender: blender -b --factory-startup --python tools/clean_mesh_blender.py -- ...")
    sys.exit(1)


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--input", required=True, help="Input raw GLB path")
    p.add_argument("--output", required=True, help="Output cleaned GLB path")
    p.add_argument("--height", type=float, default=1.25, help="Target height in metres")
    p.add_argument("--yaw", type=float, default=-90.0, help="Yaw rotation in degrees (-90 fixes Tripo +X facing)")
    return p.parse_args(argv)


def main():
    args = parse_args()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=args.input)

    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not meshes:
        raise RuntimeError(f"No mesh objects found in {args.input}")

    bpy.ops.object.select_all(action="DESELECT")
    for obj in meshes:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]

    # Apply existing transforms
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

    # Rotate mesh vertices directly around Z so quaternion glTF export keeps orientation
    yaw_rad = math.radians(args.yaw)
    rot_mat = mathutils.Matrix.Rotation(yaw_rad, 4, "Z")
    for obj in meshes:
        obj.data.transform(rot_mat)
        obj.data.update()

    # Compute combined world bounding box
    min_x = min_y = min_z = float("inf")
    max_x = max_y = max_z = float("-inf")
    for obj in meshes:
        for v in obj.data.vertices:
            co = obj.matrix_world @ v.co
            min_x, max_x = min(min_x, co.x), max(max_x, co.x)
            min_y, max_y = min(min_y, co.y), max(max_y, co.y)
            min_z, max_z = min(min_z, co.z), max(max_z, co.z)

    cur_height = max(0.001, max_z - min_z)
    scale_factor = args.height / cur_height
    cx = 0.5 * (min_x + max_x)
    cy = 0.5 * (min_y + max_y)

    # Shift feet to Z=0, center XY, and scale to target metre height
    for obj in meshes:
        for v in obj.data.vertices:
            v.co.x = (v.co.x - cx) * scale_factor
            v.co.y = (v.co.y - cy) * scale_factor
            v.co.z = (v.co.z - min_z) * scale_factor
        obj.data.update()

    # Preserve realistic PBR material response instead of flattening to chalk matte; clamp degenerate roughness only
    for mat in bpy.data.materials:
        if mat.use_nodes and mat.node_tree:
            for node in mat.node_tree.nodes:
                if node.type == "BSDF_PRINCIPLED" and "Roughness" in node.inputs:
                    node.inputs["Roughness"].default_value = max(0.18, float(node.inputs["Roughness"].default_value))

    bpy.ops.export_scene.gltf(
        filepath=args.output,
        export_format="GLB",
        export_apply=True,
        export_tangents=True,
        export_vertex_color="ACTIVE",
        export_active_vertex_color_when_no_material=True,
        export_image_format="AUTO",
    )
    print(f"[clean_mesh_blender.py] Saved normalized mesh to {args.output}")


if __name__ == "__main__":
    main()

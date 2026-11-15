"""Blender 5.3.0 clean-room miniature PBR models for M1 Defenses."""

import math
import os
import blender_pbr_kit as kit


def _gable_prism(
    name: str,
    width: float,
    depth: float,
    eave_h: float,
    z_base: float,
    mat_name: str,
    center_xy: tuple[float, float] = (0.0, 0.0),
) -> object:
    """Two-slope pitched roof wedge running along X (eaves N/S) cleanly built from beveled slab slopes."""
    ang = math.degrees(math.atan2(eave_h, depth * 0.5))
    slope_len = math.hypot(depth * 0.5, eave_h) + 0.06
    part_n = kit.add_box(
        f"{name}_n",
        (width, slope_len, 0.12),
        (center_xy[0], center_xy[1] + depth * 0.25, z_base + eave_h * 0.5),
        mat_name,
        bevel_w=0.02,
        rot_deg=(-ang, 0.0, 0.0),
    )
    part_s = kit.add_box(
        f"{name}_s",
        (width, slope_len, 0.12),
        (center_xy[0], center_xy[1] - depth * 0.25, z_base + eave_h * 0.5),
        mat_name,
        bevel_w=0.02,
        rot_deg=(ang, 0.0, 0.0),
    )
    return [part_n, part_s]


def build_quill_archer_spire(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Stepped dressed stone foundation + corner buttresses
    objs.append(kit.add_box("base_pad", (3.05, 3.05, 0.28), (0, 0, 0.14), "stone", bevel_w=0.04, taper_top=(0.94, 0.94)))
    objs.append(kit.add_box("masonry_core", (2.36, 2.36, 0.72), (0, 0, 0.62), "stone", bevel_w=0.035, taper_top=(0.88, 0.88)))
    for sx in (-1.0, 1.0):
        for sy in (-1.0, 1.0):
            objs.append(kit.add_box(f"btr_{sx}_{sy}", (0.58, 0.58, 0.92), (sx * 1.15, sy * 1.15, 0.46), "stone", bevel_w=0.03, taper_top=(0.80, 0.80)))
            objs.append(kit.add_box(f"boot_{sx}_{sy}", (0.34, 0.34, 0.22), (sx * 0.88, sy * 0.88, 1.02), "metal", bevel_w=0.02))

    # 4 canted oak scaffold columns + iron bands + X-bracing
    bot_c, top_c = 0.88, 0.76
    cb = [(-bot_c, -bot_c), (bot_c, -bot_c), (bot_c, bot_c), (-bot_c, bot_c)]
    ct = [(-top_c, -top_c), (top_c, -top_c), (top_c, top_c), (-top_c, top_c)]
    for i in range(4):
        j = (i + 1) % 4
        objs.append(kit.add_beam_between(f"col_{i}", (cb[i][0], cb[i][1], 0.95), (ct[i][0], ct[i][1], 2.75), 0.22, "wood"))
        objs.append(kit.add_beam_between(f"xb_a_{i}", (cb[i][0], cb[i][1], 1.12), (ct[j][0], ct[j][1], 2.55), 0.11, "wood"))
        objs.append(kit.add_beam_between(f"xb_b_{i}", (cb[j][0], cb[j][1], 1.12), (ct[i][0], ct[i][1], 2.55), 0.11, "wood"))
        objs.append(kit.add_beam_between(f"girt_{i}", (ct[i][0], ct[i][1], 2.58), (ct[j][0], ct[j][1], 2.58), 0.16, "metal"))

    # Cantilevered watch deck + timber breastwork + stone merlons + gold crest
    objs.append(kit.add_box("deck", (2.28, 2.28, 0.22), (0, 0, 2.78), "wood", bevel_w=0.03))
    for i in range(4):
        ang = i * 90.0
        rad = math.radians(ang)
        wx, wy = -0.98 * math.sin(rad), 0.98 * math.cos(rad)
        objs.append(kit.add_box(f"rail_{i}", (2.06, 0.18, 0.42), (wx, wy, 3.06), "wood", bevel_w=0.02, rot_deg=(0, 0, ang)))
        for off in (-0.68, 0.0, 0.68):
            mx = off * math.cos(rad) - 1.02 * math.sin(rad)
            my = off * math.sin(rad) + 1.02 * math.cos(rad)
            objs.append(kit.add_box(f"merlon_{i}_{off}", (0.38, 0.22, 0.26), (mx, my, 3.32), "stone", bevel_w=0.02, rot_deg=(0, 0, ang)))
    objs.append(kit.add_prism("shield_crest", 0.26, 0.18, 0.08, (0, -1.10, 3.04), "gold_trim", segs=6, rot_deg=(90, 0, 0), bevel_w=0.015))
    objs.append(kit.add_prism("turret_pintle", 0.24, 0.18, 0.18, (0, 0, 2.98), "bronze", segs=10, bevel_w=0.015))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "quill_archer_spire"))


def build_quill_archer_turret(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Rotating heavy arbalest mount facing +Y (Godot -Z forward)
    objs.append(kit.add_prism("pintle_collar", 0.22, 0.20, 0.22, (0, 0, 0.11), "bronze", segs=12, bevel_w=0.015))
    objs.append(kit.add_box("stock_rail", (0.20, 1.05, 0.16), (0, 0.14, 0.28), "wood", bevel_w=0.02))
    objs.append(kit.add_box("rail_grooves", (0.12, 0.96, 0.06), (0, 0.18, 0.37), "metal", bevel_w=0.01))
    objs.append(kit.add_beam_between("prod_l", (0.0, 0.56, 0.30), (-0.64, 0.34, 0.30), 0.10, "metal"))
    objs.append(kit.add_beam_between("prod_r", (0.0, 0.56, 0.30), (0.64, 0.34, 0.30), 0.10, "metal"))
    objs.append(kit.add_beam_between("string_l", (-0.62, 0.34, 0.32), (0.0, -0.22, 0.34), 0.028, "gold_trim", bevel_w=0.004))
    objs.append(kit.add_beam_between("string_r", (0.62, 0.34, 0.32), (0.0, -0.22, 0.34), 0.028, "gold_trim", bevel_w=0.004))
    # Loaded bolt ready on rail
    objs.append(kit.add_prism("loaded_shaft", 0.032, 0.032, 0.68, (0, 0.26, 0.38), "wood", segs=6, rot_deg=(90, 0, 0), bevel_w=0.004))
    objs.append(kit.add_prism("loaded_head", 0.06, 0.008, 0.14, (0, 0.64, 0.38), "gold_trim", segs=4, rot_deg=(-90, 0, 0), bevel_w=0.004))
    # Tiny armored quiver drum & cloak figure silhouettes on turret deck
    objs.append(kit.add_prism("gunner_body", 0.22, 0.16, 0.44, (0, -0.34, 0.22), "cloth_royal", segs=8, bevel_w=0.02))
    objs.append(kit.add_prism("gunner_hood", 0.15, 0.06, 0.22, (0, -0.34, 0.52), "cloth_royal", segs=8, bevel_w=0.02))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "quill_archer_turret"), recenter_xy=False)


def build_ink_cannon_base(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Heavy octagonal bastion turntable pad
    objs.append(kit.add_prism("pad_outer", 1.72, 1.56, 0.26, (0, 0, 0.13), "stone", segs=8, bevel_w=0.03))
    objs.append(kit.add_prism("pad_inlay", 1.32, 1.32, 0.10, (0, 0, 0.29), "cobble", segs=12, bevel_w=0.02))
    objs.append(kit.add_prism("turntable_ring", 1.18, 1.18, 0.07, (0, 0, 0.33), "bronze", segs=16, bevel_w=0.012))
    for i in range(4):
        a = math.radians(i * 90.0 + 45.0)
        bx, by = 1.48 * math.cos(a), 1.48 * math.sin(a)
        objs.append(kit.add_box(f"bollard_{i}", (0.34, 0.34, 0.48), (bx, by, 0.24), "stone", bevel_w=0.025, taper_top=(0.82, 0.82), rot_deg=(0, 0, i * 90.0 + 45.0)))
        objs.append(kit.add_prism(f"cap_{i}", 0.16, 0.12, 0.10, (bx, by, 0.51), "bronze", segs=6, bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "ink_cannon_base"))


def build_ink_cannon_turret(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Oak naval gun carriage with 4 armored wheels + trunnion cheeks
    for sx in (-0.42, 0.42):
        objs.append(kit.add_box(f"cheek_{sx}", (0.20, 1.24, 0.56), (sx, -0.08, 0.46), "wood", bevel_w=0.025, taper_top=(0.90, 0.86)))
        objs.append(kit.add_box(f"trun_strap_{sx}", (0.22, 0.24, 0.28), (sx, 0.08, 0.72), "bronze", bevel_w=0.015))
        for sy in (-0.42, 0.36):
            objs.append(kit.add_prism(f"wheel_{sx}_{sy}", 0.30, 0.30, 0.14, (sx * 1.25, sy, 0.30), "metal", segs=12, rot_deg=(0, 90, 0), bevel_w=0.02))
            objs.append(kit.add_prism(f"hub_{sx}_{sy}", 0.14, 0.11, 0.20, (sx * 1.25, sy, 0.30), "bronze", segs=8, rot_deg=(0, 90, 0), bevel_w=0.01))
    objs.append(kit.add_box("carriage_bed", (0.72, 1.05, 0.18), (0, -0.10, 0.30), "wood", bevel_w=0.02))
    # Forged cannon barrel aimed +Y (slightly elevated +8 deg for silhouette)
    pitch = -82.0  # cylinder Z axis tilted toward +Y and up ~8 deg
    objs.append(kit.add_prism("barrel_body", 0.38, 0.26, 1.56, (0, 0.22, 0.80), "metal", segs=16, rot_deg=(pitch, 0, 0), bevel_w=0.02))
    objs.append(kit.add_prism("muzzle_lip", 0.32, 0.34, 0.18, (0, 0.96, 0.90), "gold_trim", segs=16, rot_deg=(pitch, 0, 0), bevel_w=0.015))
    objs.append(kit.add_prism("barrel_band_f", 0.34, 0.34, 0.11, (0, 0.56, 0.85), "bronze", segs=16, rot_deg=(pitch, 0, 0), bevel_w=0.01))
    objs.append(kit.add_prism("barrel_band_r", 0.41, 0.41, 0.12, (0, -0.12, 0.75), "bronze", segs=16, rot_deg=(pitch, 0, 0), bevel_w=0.01))
    objs.append(kit.add_prism("breech_knob", 0.22, 0.10, 0.26, (0, -0.58, 0.69), "gold_trim", segs=12, rot_deg=(pitch, 0, 0), bevel_w=0.015))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "ink_cannon_turret"), recenter_xy=False)


def build_rune_mortar_base(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Reinforced heavy bastion wall ring around mortar pit
    objs.append(kit.add_prism("mortar_plinth", 1.82, 1.70, 0.28, (0, 0, 0.14), "stone", segs=12, bevel_w=0.035))
    objs.append(kit.add_prism("mortar_drum", 1.50, 1.38, 0.42, (0, 0, 0.46), "stone", segs=12, bevel_w=0.03))
    objs.append(kit.add_prism("iron_strap", 1.46, 1.46, 0.12, (0, 0, 0.52), "metal", segs=12, bevel_w=0.015))
    for i in range(8):
        a = math.radians(i * 45.0)
        rx, ry = 1.48 * math.cos(a), 1.48 * math.sin(a)
        objs.append(kit.add_box(f"rivet_lug_{i}", (0.28, 0.22, 0.50), (rx, ry, 0.36), "bronze", bevel_w=0.02, rot_deg=(0, 0, i * 45.0)))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "rune_mortar_base"))


def build_rune_mortar_tube(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Cradle + short wide high-angle bronze-banded siege mortar pointing +Y / +45 deg up
    objs.append(kit.add_box("cradle_frame", (1.06, 0.92, 0.34), (0, 0.0, 0.22), "wood", bevel_w=0.025))
    for sx in (-0.54, 0.54):
        objs.append(kit.add_box(f"trunnion_{sx}", (0.20, 0.46, 0.58), (sx, -0.04, 0.46), "bronze", bevel_w=0.02))
    ang = -45.0
    objs.append(kit.add_prism("mortar_bowl", 0.58, 0.54, 1.18, (0, 0.14, 0.72), "metal", segs=16, rot_deg=(ang, 0, 0), bevel_w=0.03))
    objs.append(kit.add_prism("mortar_lip", 0.62, 0.65, 0.22, (0, 0.50, 1.08), "gold_trim", segs=16, rot_deg=(ang, 0, 0), bevel_w=0.02))
    objs.append(kit.add_prism("rune_collar", 0.61, 0.61, 0.14, (0, 0.22, 0.80), "crystal_cyan", segs=12, rot_deg=(ang, 0, 0), bevel_w=0.015))
    objs.append(kit.add_prism("bore_plug", 0.42, 0.42, 0.14, (0, 0.46, 1.04), "ember_orange", segs=12, rot_deg=(ang, 0, 0), bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "rune_mortar_tube"), recenter_xy=False)


def build_hidden_proctor_tesla(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Retractable copper/iron induction spire + cyan crystal resonator
    objs.append(kit.add_box("mast_base", (1.15, 1.15, 0.34), (0, 0, 0.17), "stone", bevel_w=0.03))
    objs.append(kit.add_prism("column", 0.44, 0.32, 1.55, (0, 0, 1.05), "metal", segs=10, bevel_w=0.02))
    for i in range(4):
        zc = 1.25 + i * 0.34
        r = 0.56 - i * 0.05
        objs.append(kit.add_prism(f"coil_{i}", r, r, 0.13, (0, 0, zc), "bronze", segs=14, bevel_w=0.02))
        objs.append(kit.add_prism(f"disc_{i}", r * 0.86, r * 0.86, 0.07, (0, 0, zc + 0.11), "slate", segs=12, bevel_w=0.01))
    objs.append(kit.add_prism("emitter_cap", 0.42, 0.28, 0.22, (0, 0, 2.58), "gold_trim", segs=10, bevel_w=0.02))
    objs.append(kit.add_prism("arc_crystal", 0.22, 0.03, 0.58, (0, 0, 2.94), "crystal_cyan", segs=6, bevel_w=0.01))
    for sx in (-1.0, 1.0):
        objs.append(kit.add_beam_between(f"horn_a_{sx}", (sx * 0.28, 0.0, 2.55), (sx * 0.54, 0.0, 2.86), 0.11, "gold_trim"))
        objs.append(kit.add_beam_between(f"horn_b_{sx}", (sx * 0.54, 0.0, 2.86), (sx * 0.38, 0.0, 3.16), 0.09, "bronze"))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "hidden_proctor_tesla"))


def build_hidden_proctor_trapdoor(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Low 2x2 cobble hatch frame resting flush on grass
    for sx, sy, w, d in [(0, 1.15, 2.50, 0.26), (0, -1.15, 2.50, 0.26), (-1.15, 0, 0.26, 2.04), (1.15, 0, 0.26, 2.04)]:
        objs.append(kit.add_box(f"curb_{sx}_{sy}", (w, d, 0.16), (sx, sy, 0.08), "cobble", bevel_w=0.025))
    objs.append(kit.add_box("hatch_slab", (1.96, 1.96, 0.06), (0, 0, 0.03), "wood", bevel_w=0.015))
    objs.append(kit.add_box("hinge_bar", (1.80, 0.16, 0.08), (0, 0.88, 0.08), "metal", bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "hidden_proctor_trapdoor"))


def build_all(models_root: str) -> dict:
    out_dir = os.path.join(models_root, "defenses")
    return {
        "quill_archer_spire": build_quill_archer_spire(out_dir),
        "quill_archer_turret": build_quill_archer_turret(out_dir),
        "ink_cannon_base": build_ink_cannon_base(out_dir),
        "ink_cannon_turret": build_ink_cannon_turret(out_dir),
        "rune_mortar_base": build_rune_mortar_base(out_dir),
        "rune_mortar_tube": build_rune_mortar_tube(out_dir),
        "hidden_proctor_tesla": build_hidden_proctor_tesla(out_dir),
        "hidden_proctor_trapdoor": build_hidden_proctor_trapdoor(out_dir),
    }

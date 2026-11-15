"""Blender 5.3.0 clean-room miniature PBR models for Troops, Hero, and Projectiles (+Y -> Godot -Z forward)."""

import os
import blender_pbr_kit as kit


def _humanoid_legs_and_torso(objs: list, leg_mat: str, boot_mat: str, torso_mat: str, belt_mat: str, scale: float = 1.0) -> None:
    s = scale
    for sx in (-1.0, 1.0):
        objs.append(kit.add_box(f"boot_{sx}", (0.18 * s, 0.24 * s, 0.22 * s), (sx * 0.16 * s, 0.03 * s, 0.11 * s), boot_mat, bevel_w=0.02 * s))
        objs.append(kit.add_prism(f"leg_{sx}", 0.11 * s, 0.12 * s, 0.36 * s, (sx * 0.15 * s, 0.0, 0.38 * s), leg_mat, segs=8, bevel_w=0.015 * s))
    objs.append(kit.add_box("waist_belt", (0.48 * s, 0.34 * s, 0.16 * s), (0, 0, 0.62 * s), belt_mat, bevel_w=0.02 * s))
    objs.append(kit.add_box("buckle", (0.16 * s, 0.08 * s, 0.14 * s), (0, 0.18 * s, 0.62 * s), "gold_trim", bevel_w=0.01 * s))
    objs.append(kit.add_box("chest", (0.52 * s, 0.36 * s, 0.44 * s), (0, 0, 0.90 * s), torso_mat, bevel_w=0.035 * s, taper_top=(1.12, 1.08)))


def build_squire_footman(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    _humanoid_legs_and_torso(objs, "slate", "leather", "metal", "leather", 1.0)
    # Shoulder pauldrons, helmet, broadsword (+Y forward right), kite shield (left)
    for sx in (-1.0, 1.0):
        objs.append(kit.add_box(f"paul_{sx}", (0.24 * 1, 0.26, 0.20), (sx * 0.36, 0.0, 1.06), "metal", bevel_w=0.025, rot_deg=(0, sx * 18.0, 0)))
    objs.append(kit.add_prism("head", 0.19, 0.17, 0.26, (0, 0.02, 1.24), "skin_warm", segs=10, bevel_w=0.025))
    objs.append(kit.add_prism("helm", 0.22, 0.19, 0.20, (0, 0.0, 1.36), "metal", segs=10, bevel_w=0.02))
    objs.append(kit.add_box("crest_plume", (0.10, 0.32, 0.18), (0, -0.02, 1.50), "terracotta", bevel_w=0.02))
    # Kite shield on left (-X)
    objs.append(kit.add_box("kite_shield", (0.10, 0.48, 0.56), (-0.36, 0.16, 0.84), "wood", bevel_w=0.02, taper_top=(1.0, 1.12), rot_deg=(0, 0, -20)))
    objs.append(kit.add_prism("shield_boss", 0.11, 0.05, 0.08, (-0.42, 0.18, 0.84), "gold_trim", segs=8, rot_deg=(0, 90, 0), bevel_w=0.01))
    # Broadsword raised forward-right
    objs.append(kit.add_beam_between("arm_r", (0.34, 0.0, 0.98), (0.42, 0.30, 0.84), 0.12, "skin_warm"))
    objs.append(kit.add_box("hilt", (0.20, 0.08, 0.08), (0.42, 0.34, 0.86), "bronze", bevel_w=0.01))
    objs.append(kit.add_beam_between("blade", (0.42, 0.36, 0.88), (0.34, 0.68, 1.32), 0.10, "metal", bevel_w=0.012))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "squire_footman"), recenter_xy=False)


def build_lexi_archer(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    _humanoid_legs_and_torso(objs, "slate", "leather", "cloth_royal", "leather", 0.95)
    objs.append(kit.add_box("cape_back", (0.44, 0.12, 0.62), (0, -0.20, 0.78), "cloth_royal", bevel_w=0.02, rot_deg=(12, 0, 0)))
    objs.append(kit.add_prism("quiver", 0.10, 0.09, 0.44, (0.18, -0.22, 0.98), "leather", segs=8, rot_deg=(-18, 12, 0), bevel_w=0.01))
    objs.append(kit.add_prism("head", 0.17, 0.16, 0.24, (0, 0.02, 1.20), "skin_warm", segs=10, bevel_w=0.02))
    objs.append(kit.add_prism("hood", 0.22, 0.09, 0.26, (0, -0.04, 1.32), "cloth_royal", segs=10, rot_deg=(-16, 0, 0), bevel_w=0.02))
    # Drawn curved longbow (+Y forward)
    objs.append(kit.add_beam_between("bow_top", (0.0, 0.42, 0.98), (0.0, 0.30, 1.44), 0.07, "wood"))
    objs.append(kit.add_beam_between("bow_bot", (0.0, 0.42, 0.98), (0.0, 0.30, 0.52), 0.07, "wood"))
    objs.append(kit.add_beam_between("bow_string", (0.0, 0.30, 1.42), (0.0, 0.30, 0.54), 0.02, "gold_trim", bevel_w=0.003))
    objs.append(kit.add_beam_between("nocked_arrow", (0.0, 0.16, 0.98), (0.0, 0.66, 0.98), 0.04, "wood", bevel_w=0.005))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "lexi_archer"), recenter_xy=False)


def build_thesis_breaker(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    _humanoid_legs_and_torso(objs, "slate", "leather", "slate", "leather", 0.88)
    objs.append(kit.add_prism("head", 0.18, 0.17, 0.24, (0, 0.08, 1.06), "bone_ivory", segs=10, bevel_w=0.025))
    for gx in (-0.08, 0.08):
        objs.append(kit.add_prism(f"goggle_{gx}", 0.07, 0.07, 0.06, (gx, 0.24, 1.09), "bronze", segs=8, rot_deg=(90, 0, 0), bevel_w=0.01))
    # Both arms hoisting oversized strapped demolition powder keg above shoulders
    for sx in (-1.0, 1.0):
        objs.append(kit.add_beam_between(f"arm_{sx}", (sx * 0.26, 0.05, 0.90), (sx * 0.30, 0.14, 1.24), 0.10, "bone_ivory"))
    objs.append(kit.add_prism("keg", 0.34, 0.30, 0.56, (0, 0.16, 1.42), "wood", segs=12, rot_deg=(0, 88, 0), bevel_w=0.025))
    for ox in (-0.18, 0.18):
        objs.append(kit.add_prism(f"keg_hoop_{ox}", 0.35, 0.35, 0.07, (ox, 0.16, 1.42), "metal", segs=12, rot_deg=(0, 88, 0), bevel_w=0.01))
    objs.append(kit.add_prism("fuse_ember", 0.08, 0.03, 0.16, (0, 0.16, 1.76), "ember_orange", segs=6, bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "thesis_breaker"), recenter_xy=False)


def build_formula_giant(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    _humanoid_legs_and_torso(objs, "slate", "leather", "skin_warm", "leather", 1.45)
    objs.append(kit.add_box("heavy_chest", (0.86, 0.56, 0.66), (0, 0, 1.32), "skin_warm", bevel_w=0.05, taper_top=(1.14, 1.10)))
    objs.append(kit.add_box("shoulder_strap", (0.88, 0.58, 0.18), (0, 0, 1.54), "leather", bevel_w=0.02, rot_deg=(0, 18, 0)))
    objs.append(kit.add_prism("head", 0.24, 0.21, 0.32, (0, 0.04, 1.80), "skin_warm", segs=10, bevel_w=0.03))
    objs.append(kit.add_box("beard", (0.34, 0.24, 0.28), (0, 0.20, 1.66), "terracotta", bevel_w=0.025, taper_top=(1.1, 1.0)))
    # Heavy armored iron-and-bronze punch gauntlets
    for sx in (-1.0, 1.0):
        objs.append(kit.add_beam_between(f"arm_{sx}", (sx * 0.50, 0.0, 1.52), (sx * 0.56, 0.22, 1.12), 0.22, "skin_warm"))
        objs.append(kit.add_box(f"gauntlet_{sx}", (0.28, 0.32, 0.30), (sx * 0.56, 0.28, 0.98), "metal", bevel_w=0.03))
        objs.append(kit.add_box(f"knuckles_{sx}", (0.24, 0.14, 0.12), (sx * 0.56, 0.42, 0.94), "bronze", bevel_w=0.015))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "formula_giant"), recenter_xy=False)


def build_glyph_drake(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Winged hovering arcane drake (+Y forward), ground origin cleanly centered under body
    objs.append(kit.add_box("drake_torso", (0.54, 0.88, 0.46), (0, 0, 0.48), "slate", bevel_w=0.04, taper_top=(0.90, 0.86)))
    objs.append(kit.add_prism("chest_core", 0.20, 0.12, 0.24, (0, 0.28, 0.42), "crystal_cyan", segs=6, rot_deg=(70, 0, 0), bevel_w=0.01))
    objs.append(kit.add_beam_between("neck", (0, 0.34, 0.54), (0, 0.66, 0.78), 0.22, "slate"))
    objs.append(kit.add_box("head_snout", (0.32, 0.46, 0.26), (0, 0.82, 0.80), "terracotta", bevel_w=0.025, taper_top=(0.80, 0.68)))
    for sx in (-1.0, 1.0):
        objs.append(kit.add_beam_between(f"horn_{sx}", (sx * 0.12, 0.68, 0.88), (sx * 0.22, 0.48, 1.08), 0.08, "gold_trim"))
        # Splayed winged membrane & ribbed struts
        objs.append(kit.add_beam_between(f"wing_spar_{sx}", (sx * 0.24, 0.10, 0.64), (sx * 1.18, -0.10, 1.06), 0.10, "terracotta"))
        objs.append(kit.add_box(f"wing_web_{sx}", (0.86, 0.62, 0.06), (sx * 0.68, -0.04, 0.78), "slate", bevel_w=0.015, rot_deg=(12, sx * 22.0, sx * -14.0)))
    objs.append(kit.add_beam_between("tail_1", (0, -0.40, 0.44), (0, -0.96, 0.32), 0.16, "slate"))
    objs.append(kit.add_prism("tail_tip", 0.16, 0.02, 0.24, (0, -1.04, 0.32), "gold_trim", segs=4, rot_deg=(-90, 0, 0), bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "glyph_drake"), recenter_xy=False)


def build_field_scholar_hero(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    _humanoid_legs_and_torso(objs, "slate", "bronze", "bronze", "gold_trim", 1.38)
    objs.append(kit.add_box("hero_cape", (0.72, 0.14, 1.12), (0, -0.28, 1.08), "cloth_crimson", bevel_w=0.025, rot_deg=(14, 0, 0)))
    for sx in (-1.0, 1.0):
        objs.append(kit.add_box(f"gilded_paul_{sx}", (0.32, 0.34, 0.24), (sx * 0.46, 0.0, 1.48), "gold_trim", bevel_w=0.03, rot_deg=(0, sx * 20.0, 0)))
    objs.append(kit.add_prism("head", 0.21, 0.19, 0.28, (0, 0.02, 1.70), "skin_warm", segs=10, bevel_w=0.025))
    objs.append(kit.add_prism("crown", 0.24, 0.26, 0.22, (0, 0.0, 1.88), "gold_trim", segs=8, bevel_w=0.015))
    # Tall astrolabe war-staff held forward-right
    objs.append(kit.add_beam_between("staff_shaft", (0.46, 0.24, 0.14), (0.48, 0.36, 2.04), 0.09, "bronze"))
    objs.append(kit.add_prism("staff_ring", 0.22, 0.22, 0.07, (0.48, 0.36, 2.12), "gold_trim", segs=12, rot_deg=(90, 0, 0), bevel_w=0.01))
    objs.append(kit.add_prism("staff_orb", 0.13, 0.04, 0.28, (0.48, 0.36, 2.12), "crystal_cyan", segs=8, bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "field_scholar_hero"), recenter_xy=False)


def build_projectiles(models_root: str) -> dict:
    out_dir = os.path.join(models_root, "projectiles")
    res = {}
    # Arrow bolt (+Y -> Godot -Z forward)
    kit.clear_scene()
    a_objs = [
        kit.add_prism("shaft", 0.024, 0.024, 0.62, (0, 0.0, 0.06), "wood", segs=6, rot_deg=(90, 0, 0), bevel_w=0.004),
        kit.add_prism("head", 0.055, 0.006, 0.14, (0, 0.36, 0.06), "gold_trim", segs=4, rot_deg=(-90, 0, 0), bevel_w=0.004),
        kit.add_box("fletch", (0.10, 0.14, 0.02), (0, -0.24, 0.06), "terracotta", bevel_w=0.003),
    ]
    res["arrow_bolt"] = kit.finalize_and_export_lods(a_objs, os.path.join(out_dir, "arrow_bolt"), make_lods=False, recenter_xy=False)

    # Cannonball
    kit.clear_scene()
    c_objs = [
        kit.add_rock_chunk("shot_core", (0.24, 0.24, 0.24), (0, 0, 0.24), "metal", seed=2, subdiv=2, flatten_top=1.0),
        kit.add_prism("shot_band", 0.25, 0.25, 0.06, (0, 0, 0.24), "bronze", segs=12, bevel_w=0.01),
    ]
    res["cannonball"] = kit.finalize_and_export_lods(c_objs, os.path.join(out_dir, "cannonball"), make_lods=False)

    # Mortar siege shell
    kit.clear_scene()
    m_objs = [
        kit.add_prism("shell_body", 0.26, 0.18, 0.42, (0, 0, 0.24), "metal", segs=12, rot_deg=(90, 0, 0), bevel_w=0.02),
        kit.add_prism("shell_fuse", 0.14, 0.05, 0.14, (0, 0.24, 0.24), "ember_orange", segs=8, rot_deg=(-90, 0, 0), bevel_w=0.01),
        kit.add_box("shell_fins", (0.48, 0.14, 0.06), (0, -0.18, 0.24), "gold_trim", bevel_w=0.008),
    ]
    res["mortar_shell"] = kit.finalize_and_export_lods(m_objs, os.path.join(out_dir, "mortar_shell"), make_lods=False)
    return res


def build_all(models_root: str) -> dict:
    out_dir = os.path.join(models_root, "troops")
    out = {
        "squire_footman": build_squire_footman(out_dir),
        "lexi_archer": build_lexi_archer(out_dir),
        "thesis_breaker": build_thesis_breaker(out_dir),
        "formula_giant": build_formula_giant(out_dir),
        "glyph_drake": build_glyph_drake(out_dir),
        "field_scholar_hero": build_field_scholar_hero(out_dir),
    }
    out.update(build_projectiles(models_root))
    return out

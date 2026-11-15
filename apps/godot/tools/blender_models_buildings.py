"""Blender 5.3.0 clean-room miniature PBR models for Core Buildings, Storages, and Wall Tiers 0..3."""

import math
import os
import blender_pbr_kit as kit


def _add_gable_roof(objs: list, prefix: str, w: float, d: float, peak_h: float, z_eave: float, cx: float, cy: float, mat: str) -> None:
    ang = math.degrees(math.atan2(peak_h, d * 0.5))
    sl = math.hypot(d * 0.5, peak_h) + 0.16
    objs.append(kit.add_box(f"{prefix}_rn", (w, sl, 0.16), (cx, cy + d * 0.25, z_eave + peak_h * 0.5), mat, bevel_w=0.025, rot_deg=(-ang, 0, 0)))
    objs.append(kit.add_box(f"{prefix}_rs", (w, sl, 0.16), (cx, cy - d * 0.25, z_eave + peak_h * 0.5), mat, bevel_w=0.025, rot_deg=(ang, 0, 0)))
    objs.append(kit.add_box(f"{prefix}_ridge", (w + 0.18, 0.26, 0.24), (cx, cy, z_eave + peak_h + 0.04), "wood", bevel_w=0.025))
    objs.append(kit.add_box(f"{prefix}_gable_l", (0.18, d * 0.62, peak_h * 0.72), (cx - w * 0.45, cy, z_eave + peak_h * 0.34), "stone", bevel_w=0.02, taper_top=(1.0, 0.12)))
    objs.append(kit.add_box(f"{prefix}_gable_r", (0.18, d * 0.62, peak_h * 0.72), (cx + w * 0.45, cy, z_eave + peak_h * 0.34), "stone", bevel_w=0.02, taper_top=(1.0, 0.12)))


def build_townhall_tier(out_dir: str, tier: int) -> dict:
    kit.clear_scene()
    objs = []
    # Tier-progressive CoC Town Hall silhouette: timber lodge -> stone keep -> dual roof citadel -> golden apex keep
    wall_mat = ["cobble", "stone", "stone", "slate"][tier]
    roof_mat = ["terracotta", "terracotta", "slate", "terracotta"][tier]
    trim_mat = ["wood", "bronze", "gold_trim", "gold_trim"][tier]

    objs.append(kit.add_box("dais", (4.65, 4.65, 0.28), (0, 0, 0.14), "stone", bevel_w=0.04, taper_top=(0.96, 0.96)))
    objs.append(kit.add_box("courtyard_trim", (4.20, 4.20, 0.10), (0, 0, 0.32), "cobble", bevel_w=0.02))
    body_h = 1.45 + tier * 0.22
    objs.append(kit.add_box("main_keep", (3.35, 3.05, body_h), (0, 0.10, 0.35 + body_h * 0.5), wall_mat, bevel_w=0.04, taper_top=(0.94, 0.94)))

    # Corner stone pilasters & timber half-timbering belts
    for sx in (-1.0, 1.0):
        for sy in (-1.0, 1.0):
            px, py = sx * 1.62, 0.10 + sy * 1.46
            objs.append(kit.add_box(f"pil_{sx}_{sy}", (0.48, 0.48, body_h + 0.12), (px, py, 0.35 + body_h * 0.5), "stone", bevel_w=0.03, taper_top=(0.86, 0.86)))
            if tier >= 1:
                objs.append(kit.add_box(f"bcap_{sx}_{sy}", (0.42, 0.42, 0.14), (px, py, 0.46 + body_h), trim_mat, bevel_w=0.02))

    # Grand South-facing entry portico & oak iron-strapped doorway (-Y -> +Z)
    objs.append(kit.add_box("portico", (1.68, 0.82, 1.38), (0, -1.62, 0.98), "stone", bevel_w=0.03))
    objs.append(kit.add_box("oak_door", (1.02, 0.22, 1.02), (0, -1.96, 0.82), "wood", bevel_w=0.02))
    objs.append(kit.add_box("door_lintel", (1.28, 0.28, 0.20), (0, -1.94, 1.40), trim_mat, bevel_w=0.02))
    for step_i in range(3):
        objs.append(kit.add_box(f"step_{step_i}", (1.55 + step_i * 0.18, 0.32, 0.14), (0, -2.08 - step_i * 0.22, 0.30 - step_i * 0.10), "cobble", bevel_w=0.02))
    for gx in (-0.88, 0.88):
        objs.append(kit.add_prism(f"lantern_post_{gx}", 0.12, 0.08, 0.36, (gx, -2.06, 0.56), trim_mat, segs=6, bevel_w=0.01))
        objs.append(kit.add_prism(f"lantern_gem_{gx}", 0.11, 0.03, 0.22, (gx, -2.06, 0.82), "crystal_cyan", segs=6, bevel_w=0.008))

    z_eave = 0.34 + body_h
    _add_gable_roof(objs, "main", 3.68, 3.25, 1.15 + tier * 0.08, z_eave, 0.0, 0.10, roof_mat)

    if tier >= 1:
        # Side battlement wing & dormer tower
        objs.append(kit.add_box("wing_keep", (1.58, 1.42, body_h * 0.92), (-1.45, 0.12, 0.36 + body_h * 0.46), wall_mat, bevel_w=0.03))
        _add_gable_roof(objs, "wing", 1.72, 1.56, 0.72, 0.36 + body_h * 0.92, -1.45, 0.12, roof_mat)
    if tier >= 2:
        # Upper archive bell-tower with conical slate spire
        z_spire = z_eave + 1.05
        objs.append(kit.add_prism("cupola_base", 0.86, 0.78, 0.95, (0.55, 0.20, z_spire), "stone", segs=8, bevel_w=0.025))
        objs.append(kit.add_prism("cupola_roof", 1.04, 0.06, 1.15, (0.55, 0.20, z_spire + 0.96), "slate" if tier == 2 else "terracotta", segs=8, bevel_w=0.02))
        objs.append(kit.add_prism("cupola_finial", 0.14, 0.02, 0.38, (0.55, 0.20, z_spire + 1.65), "gold_trim", segs=6, bevel_w=0.01))
    if tier >= 3:
        # Royal cyan arcane beacon lantern atop highest spire + golden herald shield
        objs.append(kit.add_prism("beacon_core", 0.34, 0.14, 0.62, (-0.85, -0.35, z_eave + 1.42), "crystal_cyan", segs=6, bevel_w=0.012))
        objs.append(kit.add_prism("royal_crest", 0.42, 0.28, 0.12, (0, -1.78, 1.98), "gold_trim", segs=6, rot_deg=(90, 0, 0), bevel_w=0.018))

    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, f"core_archive_townhall_lv{tier}"))


def build_gold_mine_vault(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_box("v_plinth", (3.65, 3.65, 0.26), (0, 0, 0.13), "stone", bevel_w=0.04, taper_top=(0.94, 0.94)))
    objs.append(kit.add_box("vault_body", (2.68, 2.38, 1.56), (0, 0.22, 1.00), "stone", bevel_w=0.035, taper_top=(0.95, 0.95)))
    _add_gable_roof(objs, "vault", 2.94, 2.66, 0.92, 1.76, 0.0, 0.22, "slate")
    # Armored steel vault door & oversized spoked brass lock wheel on South face (-Y)
    objs.append(kit.add_box("door_frame", (1.42, 0.28, 1.38), (0, -0.96, 0.96), "metal", bevel_w=0.025))
    objs.append(kit.add_prism("vault_door", 0.56, 0.56, 0.16, (0, -1.08, 0.96), "metal", segs=16, rot_deg=(90, 0, 0), bevel_w=0.02))
    objs.append(kit.add_prism("lock_wheel", 0.34, 0.34, 0.12, (0, -1.16, 0.96), "gold_trim", segs=12, rot_deg=(90, 0, 0), bevel_w=0.015))
    # Stacked gold ingots overflowing a timber handcart in front right
    objs.append(kit.add_box("gold_crate", (0.96, 0.72, 0.44), (1.12, -1.06, 0.48), "wood", bevel_w=0.02))
    for i, off in enumerate([(-0.22, 0.18), (0.04, 0.22), (0.28, 0.18), (-0.08, 0.34), (0.14, 0.34)]):
        objs.append(kit.add_box(f"ingot_{i}", (0.22, 0.40, 0.14), (1.12 + off[0], -1.06, 0.58 + off[1]), "gold_trim", bevel_w=0.012, taper_top=(0.82, 0.85), rot_deg=(0, 0, i * 11.0)))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "gold_mine_vault"))


def build_focus_extractor_tank(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_prism("basin", 1.72, 1.56, 0.32, (0, 0, 0.16), "stone", segs=8, bevel_w=0.035))
    objs.append(kit.add_prism("pedestal", 1.32, 1.24, 0.34, (0, 0, 0.48), "cobble", segs=8, bevel_w=0.025))
    # Heavy glowing elixir-violet hexagonal crystal tank + 6 bronze cage ribs + dome cap
    objs.append(kit.add_prism("elixir_vessel", 0.96, 0.96, 1.48, (0, 0, 1.36), "elixir_violet", segs=8, bevel_w=0.03))
    for i in range(6):
        a = math.radians(i * 60.0)
        objs.append(kit.add_beam_between(f"rib_{i}", (1.02 * math.cos(a), 1.02 * math.sin(a), 0.56), (1.02 * math.cos(a), 1.02 * math.sin(a), 2.16), 0.14, "bronze"))
    objs.append(kit.add_prism("tank_collar", 1.12, 1.06, 0.18, (0, 0, 2.16), "bronze", segs=12, bevel_w=0.02))
    objs.append(kit.add_prism("tank_dome", 0.88, 0.24, 0.48, (0, 0, 2.46), "bronze", segs=12, bevel_w=0.025))
    objs.append(kit.add_prism("valve_wheel", 0.26, 0.26, 0.08, (0, 0, 2.76), "gold_trim", segs=10, bevel_w=0.01))
    # Side brass siphon pipes
    for sx in (-1.0, 1.0):
        objs.append(kit.add_beam_between(f"pipe_{sx}", (sx * 0.96, -0.32, 1.32), (sx * 1.28, -0.32, 0.42), 0.15, "bronze"))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "focus_extractor_tank"))


def build_study_barracks_camp(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Open muster pavilion: cobbled terrace + pitched terracotta banner roof + training dummy & shields
    objs.append(kit.add_box("camp_pad", (3.65, 3.65, 0.22), (0, 0, 0.11), "cobble", bevel_w=0.035, taper_top=(0.95, 0.95)))
    for sx in (-1.0, 1.0):
        for sy in (-1.0, 1.0):
            objs.append(kit.add_prism(f"post_{sx}_{sy}", 0.18, 0.16, 1.65, (sx * 1.28, sy * 1.18, 1.02), "wood", segs=8, bevel_w=0.015))
    objs.append(kit.add_box("back_wall", (2.68, 0.34, 1.38), (0, 1.18, 0.90), "stone", bevel_w=0.03))
    _add_gable_roof(objs, "tent", 3.10, 2.86, 1.02, 1.82, 0.0, 0.0, "terracotta")
    # Crossing crossed display blades shield rack in front courtyard
    objs.append(kit.add_prism("rack_shield", 0.38, 0.32, 0.10, (0, -0.78, 0.72), "gold_trim", segs=6, rot_deg=(78, 0, 0), bevel_w=0.015))
    objs.append(kit.add_beam_between("rack_sword_l", (-0.52, -0.75, 0.36), (0.48, -0.75, 1.18), 0.09, "metal"))
    objs.append(kit.add_beam_between("rack_sword_r", (0.52, -0.75, 0.36), (-0.48, -0.75, 1.18), 0.09, "metal"))
    # Campfire embers hearth left side
    objs.append(kit.add_prism("fire_ring", 0.52, 0.44, 0.18, (-0.98, -1.16, 0.28), "stone", segs=8, bevel_w=0.02))
    objs.append(kit.add_prism("fire_coal", 0.28, 0.06, 0.32, (-0.98, -1.16, 0.38), "ember_orange", segs=6, bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "study_barracks_camp"))


def build_clan_hall_shield_dome(out_dir: str) -> dict:
    kit.clear_scene()
    objs = []
    # Sanctuary colonnade rotunda with glowing hexagonal cyan beacon crystal
    objs.append(kit.add_prism("rotunda_step", 1.84, 1.72, 0.24, (0, 0, 0.12), "stone", segs=12, bevel_w=0.03))
    objs.append(kit.add_prism("rotunda_floor", 1.56, 1.48, 0.20, (0, 0, 0.32), "cobble", segs=12, bevel_w=0.02))
    for i in range(6):
        a = math.radians(i * 60.0 + 30.0)
        cx, cy = 1.24 * math.cos(a), 1.24 * math.sin(a)
        objs.append(kit.add_prism(f"pillar_{i}", 0.22, 0.18, 1.52, (cx, cy, 1.14), "stone", segs=8, bevel_w=0.02))
        objs.append(kit.add_box(f"pcap_{i}", (0.40, 0.40, 0.14), (cx, cy, 1.94), "bronze", bevel_w=0.015, rot_deg=(0, 0, i * 60.0 + 30.0)))
    objs.append(kit.add_prism("ring_entab", 1.42, 1.32, 0.22, (0, 0, 2.08), "slate", segs=12, bevel_w=0.025))
    objs.append(kit.add_prism("ring_trim", 1.46, 1.46, 0.08, (0, 0, 2.16), "gold_trim", segs=12, bevel_w=0.01))
    # Floating crystal core
    objs.append(kit.add_prism("beacon_bot", 0.08, 0.42, 0.56, (0, 0, 1.12), "crystal_cyan", segs=6, bevel_w=0.012))
    objs.append(kit.add_prism("beacon_top", 0.42, 0.04, 0.72, (0, 0, 1.74), "crystal_cyan", segs=6, bevel_w=0.012))
    return kit.finalize_and_export_lods(objs, os.path.join(out_dir, "clan_hall_shield_dome"))


def build_wall_tier(out_dir: str, tier: int) -> dict:
    body_mat = ["wood", "stone", "slate", "slate"][tier]
    cap_mat = ["leather", "cobble", "metal", "gold_trim"][tier]
    # Pillar
    kit.clear_scene()
    p_objs = [
        kit.add_box("p_foot", (0.98, 0.98, 0.24), (0, 0, 0.12), "stone", bevel_w=0.03, taper_top=(0.92, 0.92)),
        kit.add_box("p_shaft", (0.78 + tier * 0.03, 0.78 + tier * 0.03, 1.24 + tier * 0.10), (0, 0, 0.84 + tier * 0.05), body_mat, bevel_w=0.03, taper_top=(0.90, 0.90)),
        kit.add_box("p_cap", (0.90, 0.90, 0.22), (0, 0, 1.54 + tier * 0.10), cap_mat, bevel_w=0.025, taper_top=(0.84, 0.84)),
    ]
    if tier >= 2:
        p_objs.append(kit.add_prism("p_spike", 0.22, 0.04, 0.34, (0, 0, 1.82 + tier * 0.10), "gold_trim" if tier == 3 else "bronze", segs=6, bevel_w=0.01))
    if tier >= 3:
        p_objs.append(kit.add_box("p_rune", (0.86, 0.86, 0.14), (0, 0, 1.12), "crystal_cyan", bevel_w=0.015))
    pillar = kit.finalize_and_export_lods(p_objs, os.path.join(out_dir, f"wall_lv{tier}_pillar"))

    # Connector spanning 1.5m along X
    kit.clear_scene()
    c_objs = [
        kit.add_box("c_foot", (1.60, 0.76, 0.22), (0, 0, 0.11), "stone", bevel_w=0.025, taper_top=(0.94, 0.90)),
        kit.add_box("c_wall", (1.56, 0.56 + tier * 0.02, 1.08 + tier * 0.08), (0, 0, 0.74 + tier * 0.04), body_mat, bevel_w=0.025, taper_top=(1.0, 0.90)),
        kit.add_box("c_cope", (1.58, 0.66, 0.18), (0, 0, 1.36 + tier * 0.08), cap_mat, bevel_w=0.02, taper_top=(1.0, 0.86)),
    ]
    if tier >= 1:
        c_objs.append(kit.add_box("c_band", (1.58, 0.62, 0.12), (0, 0, 0.76), "bronze" if tier == 1 else "metal", bevel_w=0.015))
    conn = kit.finalize_and_export_lods(c_objs, os.path.join(out_dir, f"wall_lv{tier}_connector"))
    return {"pillar": pillar, "connector": conn}


def build_all(models_root: str) -> dict:
    out_dir = os.path.join(models_root, "buildings")
    out = {
        "gold_mine_vault": build_gold_mine_vault(out_dir),
        "focus_extractor_tank": build_focus_extractor_tank(out_dir),
        "study_barracks_camp": build_study_barracks_camp(out_dir),
        "clan_hall_shield_dome": build_clan_hall_shield_dome(out_dir),
    }
    for t in range(4):
        out[f"core_archive_townhall_lv{t}"] = build_townhall_tier(out_dir, t)
        w = build_wall_tier(out_dir, t)
        out[f"wall_lv{t}_pillar"] = w["pillar"]
        out[f"wall_lv{t}_connector"] = w["connector"]
    return out

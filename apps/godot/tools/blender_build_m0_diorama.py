"""Builds high-craft Clash-faithful stylized miniature PBR M0 benchmark props in Blender 5.3.0."""

import json
import math
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
if SCRIPT_DIR not in sys.path:
    sys.path.insert(0, SCRIPT_DIR)

import blender_pbr_kit as kit  # noqa: E402

OUT_DIR = os.path.abspath(os.path.join(SCRIPT_DIR, "..", "assets", "models", "m0_bench"))


def build_citadel_plinth() -> dict:
    kit.clear_scene()
    objs = []
    # Stepped ashlar dais + inlaid cobble mosaic top + recessed stair terrace
    objs.append(kit.add_box("base_step", (4.35, 4.35, 0.24), (0, 0, 0.12), "stone", bevel_w=0.04, taper_top=(0.97, 0.97)))
    objs.append(kit.add_box("mid_plinth", (3.92, 3.92, 0.42), (0, 0, 0.43), "stone", bevel_w=0.035, taper_top=(0.94, 0.94)))
    objs.append(kit.add_box("cornice", (3.88, 3.88, 0.16), (0, 0, 0.70), "stone", bevel_w=0.035))
    objs.append(kit.add_box("courtyard_inlay", (3.24, 3.24, 0.08), (0, 0, 0.80), "cobble", bevel_w=0.02))
    objs.append(kit.add_box("bronze_trim_ring", (3.44, 3.44, 0.05), (0, 0, 0.79), "bronze", bevel_w=0.01))

    # Corner battered stone buttresses + riveted bronze caps + low glowing brazier crystals
    for sx in (-1.0, 1.0):
        for sy in (-1.0, 1.0):
            bx, by = sx * 1.88, sy * 1.88
            objs.append(kit.add_box(f"buttress_{sx}_{sy}", (0.64, 0.64, 0.96), (bx, by, 0.48), "stone", bevel_w=0.035, taper_top=(0.82, 0.82)))
            objs.append(kit.add_box(f"bcap_{sx}_{sy}", (0.58, 0.58, 0.10), (bx, by, 0.99), "bronze", bevel_w=0.02))
            objs.append(kit.add_prism(f"brazier_bowl_{sx}_{sy}", 0.14, 0.22, 0.14, (bx, by, 1.11), "gold_trim", segs=8, bevel_w=0.015))
            objs.append(kit.add_prism(f"brazier_gem_{sx}_{sy}", 0.11, 0.02, 0.22, (bx, by, 1.24), "crystal_cyan", segs=6, bevel_w=0.008))

    # Battlement merlons along N/E/W perimeter and grand front steps on South (-Y)
    for side_rot in (0.0, 90.0, -90.0):
        rad = math.radians(side_rot)
        for off in (-1.05, 0.0, 1.05):
            lx, ly = off, 1.78
            wx = lx * math.cos(rad) - ly * math.sin(rad)
            wy = lx * math.sin(rad) + ly * math.cos(rad)
            objs.append(kit.add_box(f"merlon_{side_rot}_{off}", (0.52, 0.30, 0.34), (wx, wy, 0.95), "stone", bevel_w=0.025, taper_top=(0.88, 0.88), rot_deg=(0, 0, side_rot)))

    for i in range(4):
        step_y = -2.02 - i * 0.24
        step_z = 0.66 - i * 0.18
        objs.append(kit.add_box(f"stair_{i}", (1.42, 0.32, 0.18), (0, step_y, step_z), "cobble", bevel_w=0.02))
    for gx in (-0.84, 0.84):
        objs.append(kit.add_box(f"cheek_{gx}", (0.28, 1.08, 0.44), (gx, -2.26, 0.22), "stone", bevel_w=0.025, taper_top=(0.86, 0.95)))
        objs.append(kit.add_box(f"gatepost_{gx}", (0.34, 0.34, 1.18), (gx, -1.78, 0.59), "stone", bevel_w=0.03, taper_top=(0.88, 0.88)))
        objs.append(kit.add_prism(f"finial_{gx}", 0.14, 0.03, 0.22, (gx, -1.78, 1.28), "gold_trim", segs=6, bevel_w=0.01))

    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "citadel_plinth"))


def build_watch_tower() -> dict:
    kit.clear_scene()
    objs = []
    # Stone foundation plinth
    objs.append(kit.add_box("tower_base", (2.62, 2.62, 0.36), (0, 0, 0.18), "stone", bevel_w=0.04, taper_top=(0.90, 0.90)))
    objs.append(kit.add_box("tower_plinth", (2.28, 2.28, 0.52), (0, 0, 0.60), "stone", bevel_w=0.035, taper_top=(0.86, 0.86)))

    # 4 canted heavy oak corner scaffold columns + iron base boots
    bot_c = 0.92
    top_c = 0.78
    z_bot, z_top = 0.72, 2.48
    posts = [((-bot_c, -bot_c), (-top_c, -top_c)), ((bot_c, -bot_c), (top_c, -top_c)), ((bot_c, bot_c), (top_c, bot_c)), ((-bot_c, bot_c), (-top_c, top_c))]
    for idx, (bxy, txy) in enumerate(posts):
        objs.append(kit.add_beam_between(f"post_{idx}", (bxy[0], bxy[1], z_bot), (txy[0], txy[1], z_top), 0.24, "wood"))
        objs.append(kit.add_box(f"boot_{idx}", (0.32, 0.32, 0.24), (bxy[0], bxy[1], 0.84), "metal", bevel_w=0.02))

    # Crossed X timber struts on each side + horizontal girts
    corners_b = [(-bot_c, -bot_c), (bot_c, -bot_c), (bot_c, bot_c), (-bot_c, bot_c)]
    corners_t = [(-top_c, -top_c), (top_c, -top_c), (top_c, bot_c), (-top_c, bot_c)]
    for i in range(4):
        j = (i + 1) % 4
        b0, b1 = corners_b[i], corners_b[j]
        t0, t1 = corners_t[i], corners_t[j]
        objs.append(kit.add_beam_between(f"brace_a_{i}", (b0[0], b0[1], 0.95), (t1[0], t1[1], 2.28), 0.12, "wood"))
        objs.append(kit.add_beam_between(f"brace_b_{i}", (b1[0], b1[1], 0.95), (t0[0], t0[1], 2.28), 0.12, "wood"))
        objs.append(kit.add_beam_between(f"girt_{i}", (t0[0], t0[1], 2.34), (t1[0], t1[1], 2.34), 0.16, "metal"))

    # Cantilevered timber watch deck + corbel brackets
    objs.append(kit.add_box("deck_floor", (2.18, 2.18, 0.20), (0, 0, 2.52), "wood", bevel_w=0.03))
    for i in range(4):
        ang = i * 90.0
        rad = math.radians(ang)
        cx, cy = 0.0, 0.96
        wx = cx * math.cos(rad) - cy * math.sin(rad)
        wy = cx * math.sin(rad) + cy * math.cos(rad)
        objs.append(kit.add_box(f"rail_{i}", (1.96, 0.18, 0.42), (wx, wy, 2.78), "wood", bevel_w=0.02, rot_deg=(0, 0, ang)))
        for mx in (-0.64, 0.0, 0.64):
            mwx = mx * math.cos(rad) - (cy + 0.02) * math.sin(rad)
            mwy = mx * math.sin(rad) + (cy + 0.02) * math.cos(rad)
            objs.append(kit.add_box(f"tmerlon_{i}_{mx}", (0.40, 0.24, 0.24), (mwx, mwy, 3.04), "stone", bevel_w=0.02, rot_deg=(0, 0, ang)))

    # Heraldic golden shield badge on South-facing breastwork
    objs.append(kit.add_prism("herald_shield", 0.28, 0.20, 0.08, (0, -1.08, 2.76), "gold_trim", segs=6, rot_deg=(90, 0, 0), bevel_w=0.015))

    # 4 corner canopy posts + pitched slate shingle roof tiers + bronze finial
    for sx in (-0.72, 0.72):
        for sy in (-0.72, 0.72):
            objs.append(kit.add_beam_between(f"rpost_{sx}_{sy}", (sx, sy, 2.60), (sx * 0.95, sy * 0.95, 3.55), 0.13, "wood"))
    objs.append(kit.add_box("eaves_frame", (2.28, 2.28, 0.12), (0, 0, 3.56), "wood", bevel_w=0.02))
    objs.append(kit.add_prism("roof_tier1", 1.72, 1.02, 0.48, (0, 0, 3.84), "slate", segs=4, rot_deg=(0, 0, 45), bevel_w=0.025))
    objs.append(kit.add_prism("roof_tier2", 1.12, 0.18, 0.56, (0, 0, 4.30), "slate", segs=4, rot_deg=(0, 0, 45), bevel_w=0.025))
    objs.append(kit.add_prism("roof_cap", 0.22, 0.04, 0.30, (0, 0, 4.68), "gold_trim", segs=8, bevel_w=0.01))

    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "watch_tower"))


def build_wall_pillar() -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_box("pillar_kick", (1.24, 1.24, 0.24), (0, 0, 0.12), "stone", bevel_w=0.035, taper_top=(0.92, 0.92)))
    objs.append(kit.add_box("pillar_shaft", (1.08, 1.08, 1.02), (0, 0, 0.72), "stone", bevel_w=0.035, taper_top=(0.86, 0.86)))
    objs.append(kit.add_box("pillar_band", (0.98, 0.98, 0.14), (0, 0, 0.92), "bronze", bevel_w=0.02))
    objs.append(kit.add_box("pillar_cornice", (1.06, 1.06, 0.18), (0, 0, 1.32), "stone", bevel_w=0.03))
    for sx in (-0.34, 0.34):
        for sy in (-0.34, 0.34):
            objs.append(kit.add_box(f"pmerlon_{sx}_{sy}", (0.34, 0.34, 0.28), (sx, sy, 1.54), "stone", bevel_w=0.025, taper_top=(0.84, 0.84)))
    objs.append(kit.add_prism("pillar_gem", 0.14, 0.04, 0.16, (0, 0, 1.48), "gold_trim", segs=6, bevel_w=0.01))
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "wall_pillar"))


def build_wall_connector() -> dict:
    kit.clear_scene()
    objs = []
    # 1.50 m span along local X axis joining neighboring wall_pillar centers
    objs.append(kit.add_box("conn_foot", (1.50, 0.88, 0.22), (0, 0, 0.11), "stone", bevel_w=0.03, taper_top=(1.0, 0.90)))
    objs.append(kit.add_box("conn_body", (1.50, 0.74, 0.88), (0, 0, 0.65), "stone", bevel_w=0.03, taper_top=(1.0, 0.86)))
    objs.append(kit.add_box("conn_strap", (1.48, 0.68, 0.10), (0, 0, 0.82), "bronze", bevel_w=0.015))
    objs.append(kit.add_box("conn_cap", (1.50, 0.76, 0.16), (0, 0, 1.16), "stone", bevel_w=0.025))
    for mx in (-0.42, 0.42):
        for sy in (-0.22, 0.22):
            objs.append(kit.add_box(f"cmerlon_{mx}_{sy}", (0.32, 0.20, 0.22), (mx, sy, 1.34), "stone", bevel_w=0.02))
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "wall_connector"))


def build_arcane_pylon() -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_prism("pylon_step1", 1.35, 1.24, 0.22, (0, 0, 0.11), "stone", segs=8, bevel_w=0.03))
    objs.append(kit.add_prism("pylon_step2", 1.08, 0.96, 0.34, (0, 0, 0.39), "stone", segs=8, bevel_w=0.03))
    objs.append(kit.add_prism("pylon_inlay", 0.88, 0.84, 0.08, (0, 0, 0.58), "cobble", segs=8, bevel_w=0.015))
    objs.append(kit.add_prism("pylon_ring", 0.76, 0.72, 0.12, (0, 0, 0.64), "bronze", segs=12, bevel_w=0.015))
    for i in range(4):
        ang = i * 90.0 + 45.0
        rad = math.radians(ang)
        bx, by = math.cos(rad) * 0.76, math.sin(rad) * 0.76
        tx, ty = math.cos(rad) * 0.44, math.sin(rad) * 0.44
        objs.append(kit.add_beam_between(f"claw_{i}", (bx, by, 0.58), (tx, ty, 1.65), 0.16, "bronze", bevel_w=0.018))
        objs.append(kit.add_prism(f"claw_tip_{i}", 0.11, 0.03, 0.24, (tx * 0.92, ty * 0.92, 1.72), "gold_trim", segs=6, rot_deg=(0, 0, ang), bevel_w=0.01))

    # Central faceted crystal spire + 3 satellite shards
    objs.append(kit.add_prism("main_crystal_bot", 0.26, 0.40, 1.05, (0, 0, 1.28), "crystal_cyan", segs=6, bevel_w=0.018))
    objs.append(kit.add_prism("main_crystal_tip", 0.40, 0.03, 0.78, (0, 0, 2.18), "crystal_cyan", segs=6, bevel_w=0.015))
    for i, (ang, tilt, h) in enumerate([(15, 16, 0.88), (138, 21, 0.74), (258, 18, 0.80)]):
        rad = math.radians(ang)
        sx, sy = math.cos(rad) * 0.42, math.sin(rad) * 0.42
        objs.append(kit.add_prism(f"shard_{i}", 0.16, 0.03, h, (sx, sy, 0.98 + h * 0.3), "crystal_cyan", segs=6, rot_deg=(tilt * math.sin(rad), -tilt * math.cos(rad), ang), bevel_w=0.01))
    # Floating golden crown collar
    for seg in range(6):
        a0 = math.radians(seg * 60.0)
        a1 = math.radians((seg + 1) * 60.0)
        objs.append(kit.add_beam_between(f"halo_bar_{seg}", (math.cos(a0) * 0.62, math.sin(a0) * 0.62, 1.88), (math.cos(a1) * 0.62, math.sin(a1) * 0.62, 1.88), 0.075, "gold_trim"))
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "arcane_pylon"))


def build_elixir_vat() -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_box("vat_plinth", (2.60, 2.60, 0.28), (0, 0, 0.14), "stone", bevel_w=0.04, taper_top=(0.94, 0.94)))
    for i in range(4):
        ang = math.radians(i * 90.0 + 45.0)
        objs.append(kit.add_box(f"cradle_{i}", (0.36, 0.48, 0.56), (math.cos(ang) * 0.88, math.sin(ang) * 0.88, 0.48), "wood", bevel_w=0.025, rot_deg=(0, 0, i * 90.0 + 45.0)))
    # glowing elixir vessel core + oak rib staves + bronze bands
    objs.append(kit.add_prism("vat_elixir_core", 0.72, 0.78, 1.25, (0, 0, 1.05), "elixir_violet", segs=12, bevel_w=0.02))
    objs.append(kit.add_prism("vat_hoop_bot", 0.88, 0.90, 0.16, (0, 0, 0.56), "bronze", segs=12, bevel_w=0.015))
    objs.append(kit.add_prism("vat_hoop_mid", 0.94, 0.94, 0.14, (0, 0, 1.05), "gold_trim", segs=12, bevel_w=0.015))
    objs.append(kit.add_prism("vat_hoop_top", 0.88, 0.82, 0.18, (0, 0, 1.56), "bronze", segs=12, bevel_w=0.015))
    for i in range(6):
        ang = math.radians(i * 60.0)
        objs.append(kit.add_box(f"stave_{i}", (0.14, 0.18, 1.18), (math.cos(ang) * 0.86, math.sin(ang) * 0.86, 1.05), "wood", bevel_w=0.02, rot_deg=(0, 0, i * 60.0)))
    objs.append(kit.add_prism("vat_cap_rim", 0.68, 0.52, 0.24, (0, 0, 1.76), "gold_trim", segs=12, bevel_w=0.02))
    objs.append(kit.add_prism("vat_top_orb", 0.46, 0.16, 0.34, (0, 0, 1.98), "elixir_violet", segs=12, bevel_w=0.02))
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "elixir_vat"))


def build_pine_cluster() -> dict:
    kit.clear_scene()
    objs = []
    objs.append(kit.add_prism("trunk_base", 0.38, 0.24, 0.70, (0, 0, 0.35), "pine_bark", segs=7, bevel_w=0.02))
    objs.append(kit.add_prism("trunk_mid", 0.24, 0.12, 1.65, (0.04, -0.02, 1.35), "pine_bark", segs=7, rot_deg=(2.5, -1.5, 0)))
    tiers = [(1.38, 0.34, 1.18, 1.62, 12), (1.10, 0.28, 1.06, 2.38, 38), (0.82, 0.18, 0.96, 3.08, 65), (0.50, 0.04, 0.90, 3.74, 92)]
    for idx, (r1, r2, h, z, rz) in enumerate(tiers):
        objs.append(kit.add_prism(f"bough_{idx}", r1, r2, h, (0.03 * (idx % 2), -0.02 * idx, z), "pine_foliage", segs=9, bevel_w=0.03, rot_deg=(1.5 * ((idx % 2) - 0.5), 0.0, rz)))
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "pine_cluster"))


def build_cliff_boulder() -> dict:
    kit.clear_scene()
    objs = [
        kit.add_rock_chunk("rock_main", (0.98, 0.82, 0.72), (0.0, 0.0, 0.46), "rock", seed=3, subdiv=2),
        kit.add_rock_chunk("rock_side_a", (0.64, 0.56, 0.48), (0.62, -0.24, 0.30), "rock", seed=7, subdiv=2),
        kit.add_rock_chunk("rock_side_b", (0.52, 0.48, 0.38), (-0.58, 0.32, 0.24), "rock", seed=11, subdiv=2),
    ]
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "cliff_boulder"))


def build_cliff_segment() -> dict:
    kit.clear_scene()
    objs = [
        kit.add_rock_chunk("cliff_core", (2.6, 1.5, 1.25), (0.0, 0.0, 0.85), "rock", seed=17, subdiv=2, flatten_top=0.88),
        kit.add_rock_chunk("cliff_sub_l", (1.6, 1.3, 0.95), (-1.45, 0.25, 0.62), "rock", seed=23, subdiv=2, flatten_top=0.80),
        kit.add_rock_chunk("cliff_sub_r", (1.7, 1.2, 0.98), (1.42, -0.20, 0.66), "rock", seed=31, subdiv=2, flatten_top=0.82),
    ]
    return kit.finalize_and_export_lods(objs, os.path.join(OUT_DIR, "cliff_segment"))


def main() -> None:
    report = {
        "citadel_plinth": build_citadel_plinth(),
        "watch_tower": build_watch_tower(),
        "wall_pillar": build_wall_pillar(),
        "wall_connector": build_wall_connector(),
        "arcane_pylon": build_arcane_pylon(),
        "elixir_vat": build_elixir_vat(),
        "pine_cluster": build_pine_cluster(),
        "cliff_boulder": build_cliff_boulder(),
        "cliff_segment": build_cliff_segment(),
    }
    print("M0_BLENDER_BUILD_OK")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

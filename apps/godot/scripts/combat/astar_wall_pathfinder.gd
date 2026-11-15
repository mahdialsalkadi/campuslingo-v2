class_name AStarWallPathfinder
extends RefCounted
## SEC-SIM01: 40x40 A* Grid Solver (1.0 open, 12.0 wall weight, 2.0 wall-breaker, air bypass).

const DIRS: Array[Vector2i] = [
	Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1),
	Vector2i(1, 1), Vector2i(1, -1), Vector2i(-1, 1), Vector2i(-1, -1),
]


static func build_wall_map(buildings: Array) -> Dictionary:
	var walls := {}
	for b in buildings:
		if not b.get("alive", true) or String(b.get("kind", "")) != "WALL":
			continue
		var gx: int = int(b["gx"])
		var gz: int = int(b["gz"])
		var hp_ratio: float = clampf(float(b["hp"]) / maxf(1.0, float(b["max_hp"])), 0.05, 1.0)
		walls[Vector2i(gx, gz)] = {"building": b, "hp_ratio": hp_ratio}
	return walls


static func find_path(
	start: Vector2i,
	goal: Vector2i,
	wall_map: Dictionary,
	wall_cost: float = 12.0,
	ignore_walls: bool = false
) -> Array[Vector2i]:
	if start == goal:
		return [goal]
	var n: int = GridMath.GRID_SIZE
	var total: int = n * n
	var g_score := PackedFloat32Array()
	g_score.resize(total)
	g_score.fill(1e9)
	var came_from := PackedInt32Array()
	came_from.resize(total)
	came_from.fill(-1)
	var closed := PackedByteArray()
	closed.resize(total)

	var s_idx: int = start.y * n + start.x
	var g_idx: int = goal.y * n + goal.x
	g_score[s_idx] = 0.0
	var open_list: Array = [[_h(start, goal), s_idx]]

	while open_list.size() > 0:
		var best_pos: int = 0
		for i in range(1, open_list.size()):
			if float(open_list[i][0]) < float(open_list[best_pos][0]):
				best_pos = i
		var cur_pair: Array = open_list[best_pos]
		open_list.remove_at(best_pos)
		var c_idx: int = int(cur_pair[1])
		if closed[c_idx] == 1:
			continue
		if c_idx == g_idx:
			return _reconstruct(came_from, c_idx, n)
		closed[c_idx] = 1
		var cx: int = c_idx % n
		var cz: int = c_idx / n
		for d in DIRS:
			var nx: int = cx + d.x
			var nz: int = cz + d.y
			if nx < 0 or nz < 0 or nx >= n or nz >= n:
				continue
			var n_idx: int = nz * n + nx
			if closed[n_idx] == 1:
				continue
			var is_diag: bool = d.x != 0 and d.y != 0
			if is_diag and not ignore_walls:
				if wall_map.has(Vector2i(cx + d.x, cz)) or wall_map.has(Vector2i(cx, cz + d.y)):
					continue
			var step_mult: float = 1.4142 if is_diag else 1.0
			var cell_weight: float = 1.0
			var n_cell := Vector2i(nx, nz)
			if not ignore_walls and wall_map.has(n_cell):
				var winfo: Dictionary = wall_map[n_cell]
				cell_weight = wall_cost * (0.75 + 0.25 * float(winfo["hp_ratio"]))
			var cand_g: float = g_score[c_idx] + step_mult * cell_weight
			if cand_g < g_score[n_idx]:
				came_from[n_idx] = c_idx
				g_score[n_idx] = cand_g
				open_list.append([cand_g + _h(n_cell, goal), n_idx])
	return [goal]


static func first_wall_on_path(path: Array[Vector2i], wall_map: Dictionary) -> Variant:
	for c in path:
		if wall_map.has(c):
			return wall_map[c]["building"]
	return null


static func _h(a: Vector2i, b: Vector2i) -> float:
	return Vector2(float(a.x - b.x), float(a.y - b.y)).length()


static func _reconstruct(came_from: PackedInt32Array, cur: int, n: int) -> Array[Vector2i]:
	var rev: Array[Vector2i] = []
	var node: int = cur
	while node != -1:
		rev.append(Vector2i(node % n, node / n))
		node = came_from[node]
	rev.reverse()
	return rev

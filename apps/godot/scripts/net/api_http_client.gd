class_name ApiHttpClient
extends Node
## Shared async HTTP/WebSocket transport with session cookie jar for SEC-C04..C06.

const DEFAULT_BASE_URL := "http://127.0.0.1:18080"
const DEFAULT_COURSE_ID := "00000000-0000-4000-8000-000000000201"

var base_url: String = DEFAULT_BASE_URL
var cookies: Dictionary = {}
var user_id: String = ""
var course_id: String = "00000000-0000-4000-8000-000000000201"
var online: bool = false
var last_error: String = ""

static var _shared: ApiHttpClient = null


static func ensure(host: Node) -> ApiHttpClient:
	if _shared != null and is_instance_valid(_shared):
		return _shared
	var loop := Engine.get_main_loop() as SceneTree
	var tree_root := host.get_tree().root if (host != null and host.is_inside_tree()) else (loop.root if loop != null else null)
	if tree_root != null:
		var existing := tree_root.get_node_or_null("ApiHttpClient")
		if existing is ApiHttpClient:
			_shared = existing as ApiHttpClient
			return _shared
	var created := ApiHttpClient.new()
	created.name = "ApiHttpClient"
	var env_url := OS.get_environment("CAMPUSLINGO_API_URL")
	if OS.has_feature("web"):
		var origin := str(JavaScriptBridge.eval("window.location.origin", true)).trim_suffix("/")
		if origin.begins_with("http"):
			created.base_url = origin
	elif env_url != "":
		created.base_url = env_url.trim_suffix("/")
	_shared = created
	if tree_root != null:
		tree_root.call_deferred("add_child", created)
	elif host != null:
		host.add_child(created)
	return created


func set_base_url(url: String) -> void:
	base_url = url.trim_suffix("/")


func ws_url(path: String) -> String:
	var ws_base := base_url.replace("https://", "wss://").replace("http://", "ws://")
	return ws_base + path


func build_headers(has_body: bool) -> PackedStringArray:
	var hdrs := PackedStringArray()
	if has_body:
		hdrs.append("Content-Type: application/json")
	if not OS.has_feature("web") and not cookies.is_empty():
		var pairs: PackedStringArray = []
		for k in cookies.keys():
			pairs.append("%s=%s" % [String(k), String(cookies[k])])
		hdrs.append("Cookie: " + "; ".join(pairs))
	return hdrs


func request_json(method: int, path: String, payload: Variant = null) -> Dictionary:
	for _w in range(4):
		if is_inside_tree():
			break
		var loop := Engine.get_main_loop() as SceneTree
		if loop == null:
			break
		await loop.process_frame
	if not is_inside_tree():
		return {"ok": false, "status": 0, "data": null, "error": "client_not_in_tree"}
	var req := HTTPRequest.new()
	req.timeout = 4.0
	add_child(req)
	var body_str := ""
	var has_body := payload != null
	if has_body:
		body_str = JSON.stringify(payload) if typeof(payload) != TYPE_STRING else String(payload)
	var url := base_url + path
	var err := req.request(url, build_headers(has_body), method, body_str)
	if err != OK:
		req.queue_free()
		last_error = "request_init_%d" % err
		return {"ok": false, "status": 0, "data": null, "error": last_error}
	var res_arr: Array = await req.request_completed
	req.queue_free()
	var result_code := int(res_arr[0])
	var status := int(res_arr[1])
	var resp_headers: PackedStringArray = res_arr[2]
	var raw_bytes: PackedByteArray = res_arr[3]
	_absorb_cookies(resp_headers)
	var text := raw_bytes.get_string_from_utf8()
	var parsed: Variant = null
	var stripped := text.strip_edges()
	if stripped.begins_with("{") or stripped.begins_with("["):
		var jp := JSON.new()
		if jp.parse(stripped) == OK:
			parsed = jp.data
	var ok := result_code == HTTPRequest.RESULT_SUCCESS and status >= 200 and status < 300
	if ok:
		online = true
		last_error = ""
	else:
		last_error = "http_%d_res_%d" % [status, result_code]
	return {"ok": ok, "status": status, "data": parsed, "raw": text, "error": last_error}


func _absorb_cookies(resp_headers: PackedStringArray) -> void:
	for raw_h in resp_headers:
		var line := String(raw_h)
		var colon := line.find(":")
		if colon <= 0:
			continue
		if line.substr(0, colon).strip_edges().to_lower() != "set-cookie":
			continue
		var val := line.substr(colon + 1).strip_edges()
		var semi := val.find(";")
		var kv := val.substr(0, semi) if semi >= 0 else val
		var eq := kv.find("=")
		if eq > 0:
			var cname := kv.substr(0, eq).strip_edges()
			var cval := kv.substr(eq + 1).strip_edges()
			if cval == "" or kv.find("Max-Age=0") >= 0:
				cookies.erase(cname)
			else:
				cookies[cname] = cval

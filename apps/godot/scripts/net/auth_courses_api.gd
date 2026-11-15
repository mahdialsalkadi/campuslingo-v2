class_name AuthCoursesAPI
extends Node
## SEC-C04: Auth (/v1/auth/*, /v1/me) & Courses (/v1/courses, /enroll) REST slice.

const SECTION_ID := "SEC-C04"
const DEMO_EMAIL := "student@campuslingo.local"
const DEMO_PASS := "CampusLingo#2026!"

signal auth_changed(online: bool, user_id: String, course_id: String)

var _client: ApiHttpClient = null


func _ready() -> void:
	_ensure_client()


func _ensure_client() -> ApiHttpClient:
	if _client == null:
		_client = ApiHttpClient.ensure(self)
	return _client


func connect_demo_student() -> Dictionary:
	var me_res := await get_me()
	if not bool(me_res.get("ok", false)):
		var login_res := await login(DEMO_EMAIL, DEMO_PASS)
		if not bool(login_res.get("ok", false)):
			auth_changed.emit(false, "", _ensure_client().course_id)
			return login_res
	var courses_res := await list_courses()
	var cid := _ensure_client().course_id
	if bool(courses_res.get("ok", false)) and courses_res.get("data") is Array:
		var arr: Array = courses_res["data"]
		var picked := ""
		for item in arr:
			if item is Dictionary and String((item as Dictionary).get("id", "")) == ApiHttpClient.DEFAULT_COURSE_ID:
				picked = ApiHttpClient.DEFAULT_COURSE_ID
				break
		if picked == "" and not arr.is_empty() and arr[0] is Dictionary:
			picked = String((arr[0] as Dictionary).get("id", cid))
		if picked != "":
			cid = picked
			_ensure_client().course_id = cid
	auth_changed.emit(true, _ensure_client().user_id, cid)
	return {"ok": true, "user_id": _ensure_client().user_id, "course_id": cid}


func login(email: String, password: String) -> Dictionary:
	var c := _ensure_client()
	var res := await c.request_json(HTTPClient.METHOD_POST, "/v1/auth/login", {"email": email, "password": password})
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		c.user_id = String((res["data"] as Dictionary).get("user_id", ""))
		auth_changed.emit(true, c.user_id, c.course_id)
	return res


func register(email: String, password: String) -> Dictionary:
	var c := _ensure_client()
	var res := await c.request_json(HTTPClient.METHOD_POST, "/v1/auth/register", {"email": email, "password": password})
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		c.user_id = String((res["data"] as Dictionary).get("user_id", ""))
		auth_changed.emit(true, c.user_id, c.course_id)
	return res


func get_me() -> Dictionary:
	var c := _ensure_client()
	var res := await c.request_json(HTTPClient.METHOD_GET, "/v1/me")
	if bool(res.get("ok", false)) and res.get("data") is Dictionary:
		c.user_id = String((res["data"] as Dictionary).get("user_id", ""))
	return res


func logout() -> Dictionary:
	var c := _ensure_client()
	var res := await c.request_json(HTTPClient.METHOD_POST, "/v1/auth/logout")
	c.cookies.clear()
	c.user_id = ""
	c.online = false
	auth_changed.emit(false, "", c.course_id)
	return res


func list_courses() -> Dictionary:
	return await _ensure_client().request_json(HTTPClient.METHOD_GET, "/v1/courses")


func enroll_course(course_id: String = "") -> Dictionary:
	var cid := course_id if course_id != "" else _ensure_client().course_id
	return await _ensure_client().request_json(HTTPClient.METHOD_POST, "/v1/courses/%s/enroll" % cid)


func is_online() -> bool:
	return _ensure_client().online


func get_user_id() -> String:
	return _ensure_client().user_id


func get_course_id() -> String:
	return _ensure_client().course_id

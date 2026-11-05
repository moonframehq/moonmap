-- Moonmap's shell: Apple Maps' layout on Moonframe's `ui.map`. The
-- sidebar lists places and recents; the map carries Maps' controls down its
-- right edge, Look Around bottom-left and the weather bottom-right.
--
-- `ui.map` draws its own zoom buttons and credit in its top-right and
-- bottom-right corners. The map overhangs the area by OVERHANG on both
-- sides (so its middle stays the area's middle), the area clips it, and
-- the view draws Maps-style controls and the credit itself.

local TO = "provider:maps"
local OVERHANG = 72
local loading = use "loading"
local SIDEBAR = 284
local MAX_ZOOM = 19
-- How close a launch, a place or the user's position is shown. Maps.app opens
-- at about 15.2, but tiles bake their labels in: here, just before Moonframe's
-- next tile level, they stay near their full size.
local CLOSE_ZOOM = 15.8

-- What the view last drew, for `update` to zoom from and fit around.
local seen = { lat = 37.788, lon = -122.4075, zoom = CLOSE_ZOOM }

local function send(action, extra)
	local msg = extra or {}
	msg.action = action
	return { to = TO, msg = msg }
end

-- A glyph Moonframe's icon set lacks, from app/icons (see `just icons`).
-- `tone` is "blue", "white", or a boolean: the map controls' ink for a
-- dark (true) or light (false) appearance.
local function png(name, size, tone)
	local suffix = if tone == nil then "blue" elseif tone == true then "night" elseif tone == false then "ink" else tone
	return ui.image { src = "icons/" .. name .. "-" .. suffix .. ".png", width = size, height = size }
end

-- An icon name, or "png:<name>" for one of ours.
local function glyph(name, size, tone)
	if string.sub(name, 1, 4) == "png:" then return png(string.sub(name, 5), size or 14, tone or "white") end
	return ui.icon { name = name, size = "small" }
end

-- Zoom that fits places (each { lat, lon }) in about the map's free room.
local function fit(places, room_x, room_y)
	if #places == 0 then return nil end
	local function world(lat, lon)
		local r = math.rad(math.clamp(lat, -85, 85))
		return (lon + 180) / 360, (1 - math.log(math.tan(r) + 1 / math.cos(r)) / math.pi) / 2
	end
	local x0, y0 = world(places[1].lat, places[1].lon)
	local x1, y1 = x0, y0
	for _, place in places do
		local x, y = world(place.lat, place.lon)
		x0, x1, y0, y1 = math.min(x0, x), math.max(x1, x), math.min(y0, y), math.max(y1, y)
	end
	local function zoom(room, span) return if span <= 0 then 16 else math.log(room / (256 * span), 2) end
	local z = math.clamp(math.min(zoom(room_x, x1 - x0), zoom(room_y, y1 - y0)), 2, 16)
	-- The middle of the box, in degrees.
	local mx, my = (x0 + x1) / 2, (y0 + y1) / 2
	local lat = math.deg(math.atan(math.sinh(math.pi * (1 - 2 * my))))
	return { lat = lat, lon = mx * 360 - 180, zoom = math.floor(z * 4) / 4 }
end

local function fly(state, lat, lon, zoom)
	-- A nudge keeps a repeat of the same spot a change the map follows.
	state.nudge = not state.nudge
	local nudge = if state.nudge then 1e-7 else 0
	state.center = { lat + nudge, lon }
	state.zoom = math.clamp(zoom or seen.zoom, 2, MAX_ZOOM)
end

------------------------------------------------------------------------
-- Sidebar

local function badge(icon, color, size)
	size = size or 28
	return ui.row { width = size, height = size, shrink = false, align = "center", justify = "center", class = "badge", style = { background = color, color = "#ffffff", radius = size / 2 },
		glyph(icon, 15),
	}
end

local function place_row(place, msg, extra)
	return ui.row { height = 40, shrink = false, align = "center", class = "side-row", msg = msg or { action = "open", id = place.id }, to = if msg then nil else TO,
		badge(place.icon, place.color),
		ui.column { grow = true, gap = "none",
			ui.text { place.name, truncate = true, class = "side-row-title" },
			if place.subtitle ~= "" then ui.text { place.subtitle, truncate = true, class = "side-sub" } else nil,
		},
		extra,
	}
end

local function nav_row(label, icon, view, count)
	return ui.row { height = 34, shrink = false, align = "center", class = "side-row", msg = { view = view },
		ui.row { width = 22, shrink = false, justify = "center",
			if string.find(icon, "^png:") then png(string.sub(icon, 5), 19) else ui.icon { name = icon, tone = "accent" },
		},
		ui.column { grow = true, ui.text { label, class = "side-row-title" } },
		if count and count > 0 then ui.text { tostring(count), class = "side-sub" } else nil,
	}
end

local function label(text) return ui.text { text, class = "side-label" } end

local function empty(text) return ui.text { text, class = "side-empty" } end

local function back(title)
	return {
		ui.row { height = 30, shrink = false, align = "center", class = "side-back", msg = { view = "home" }, self_align = "start",
			ui.icon { name = "chevron-left", size = "small" }, ui.text { "Back" },
		},
		ui.text { title, class = "side-title" },
	}
end

local function places_list(items, children, none)
	if #items == 0 then table.insert(children, empty(none)) end
	for _, place in items do table.insert(children, place_row(place)) end
end

local function home(maps, children)
	table.insert(children, label("Places"))
	table.insert(children, nav_row("Pinned", "pin", "pinned", #(maps.pinned or {})))
	table.insert(children, nav_row("Saved Places", "png:saved", "saved", #(maps.saved or {})))
	table.insert(children, nav_row("Guides", "book-open", "guides", #(maps.guides or {})))
	table.insert(children, nav_row("Routes", "png:route", "routes", #(maps.routes or {})))
	table.insert(children, nav_row("Visited Places", "png:visited", "visited"))
	table.insert(children, nav_row("Recently Added", "png:added", "added"))
	local suggestions = maps.suggestions or {}
	if #suggestions > 0 then
		table.insert(children, label("Siri Suggestions"))
		for _, suggestion in suggestions do
			table.insert(children, place_row({ name = suggestion.title, subtitle = suggestion.subtitle, icon = "png:car", color = "#f0424e" }, { pick = suggestion.place.id, directions = true }))
		end
	end
	local recents = maps.recents or {}
	if #recents > 0 then
		table.insert(children, label("Recents"))
		for _, recent in recents do
			if recent.kind == "place" then
				table.insert(children, place_row(recent.place))
			else
				table.insert(children, place_row({ name = recent.query, subtitle = recent.subtitle, icon = "search", color = "#8e8e93" }, { msg = "query", value = recent.query, submit = true }))
			end
		end
		table.insert(children, ui.row { class = "side-link", msg = send("clear_recents").msg, to = TO, self_align = "start", ui.text { "Clear Recents" } })
	end
end

local function guides(maps, state, children)
	table.insert(children, ui.input { id = "new-guide", value = state.guide_name, placeholder = "New guide name · Enter", change = "guide_name", submit = "guide_new", class = "side-field" })
	local list = maps.guides or {}
	if #list == 0 then table.insert(children, empty("Collect places into guides from a place's card.")) end
	for _, guide in list do
		local count = #guide.places
		table.insert(children, place_row({ name = guide.name, subtitle = tostring(count) .. (if count == 1 then " place" else " places"), icon = "book-open", color = "#ff9f0a" }, { view = "guide", guide = guide.id }))
	end
end

local function find_guide(maps, id)
	for _, guide in maps.guides or {} do
		if guide.id == id then return guide end
	end
	return nil
end

local MODE_ICON = { car = "png:car", walk = "png:walk", bike = "png:bike" }

local function sidebar(state, maps, app)
	local view = state.view
	local children = {}
	if maps.error and maps.error ~= "" then table.insert(children, ui.text { maps.error, class = "error" }) end
	if state.query ~= "" and (maps.query == state.query or maps.searching) and view == "home" then
		table.insert(children, label(if maps.searching then "Searching…" else "Results"))
		places_list(maps.results or {}, children, if maps.searching then "" elseif maps.online then "No results near here." else "Search needs a network connection.")
	elseif view == "pinned" then
		for _, child in back("Pinned") do table.insert(children, child) end
		places_list(maps.pinned or {}, children, "Pin places from their card to keep them here.")
	elseif view == "saved" then
		for _, child in back("Saved Places") do table.insert(children, child) end
		places_list(maps.saved or {}, children, "Save places from their card.")
	elseif view == "guides" then
		for _, child in back("Guides") do table.insert(children, child) end
		guides(maps, state, children)
	elseif view == "guide" then
		local guide = find_guide(maps, state.guide)
		if guide then
			for _, child in back(guide.name) do table.insert(children, child) end
			places_list(guide.places, children, "Add places to this guide from their card.")
			table.insert(children, ui.row { class = "side-link", msg = { delete_guide = guide.id }, self_align = "start", ui.text { "Delete Guide" } })
		else
			for _, child in back("Guide") do table.insert(children, child) end
		end
	elseif view == "routes" then
		for _, child in back("Routes") do table.insert(children, child) end
		local routes = maps.routes or {}
		if #routes == 0 then table.insert(children, empty("Save directions from a place's card.")) end
		for _, route in routes do
			table.insert(children, place_row({ name = route.to.name, subtitle = "From " .. route.from.name, icon = MODE_ICON[route.mode] or "png:car", color = "#0a84ff" }, { pick = route.to.id, directions = true, mode = route.mode },
				ui.button { icon = "trash-2", small = true, variant = "ghost", tooltip = "Remove route", msg = send("route_remove", { route = route.id }).msg, to = TO }))
		end
	elseif view == "visited" then
		for _, child in back("Visited Places") do table.insert(children, child) end
		local visited = maps.visited or {}
		if #visited == 0 then table.insert(children, empty("Places you open show up here.")) end
		for _, place in visited do
			table.insert(children, place_row(place, nil, ui.text { tostring(place.count) .. "×", class = "side-sub" }))
		end
	elseif view == "added" then
		for _, child in back("Recently Added") do table.insert(children, child) end
		places_list(maps.added or {}, children, "Places you pin, save or add to guides.")
	else
		home(maps, children)
	end
	return ui.column { grow = true, class = "side",
		ui.row { height = 52, shrink = false, align = "center", drag = true, class = "side-top",
			ui.spacer {},
			ui.row { class = "side-toggle", tooltip = "Hide Sidebar · ⌃⌘S", msg = send("sidebar", { open = false }).msg, to = TO, ui.icon { name = "panel-left" } },
		},
		ui.row { height = 34, shrink = false, align = "center", class = "side-search",
			ui.icon { name = "search", size = "small" },
			ui.column { grow = true,
				ui.input { id = "search", value = state.query, placeholder = "Search Maps", change = "query", submit = "search", focus = state.focus, class = "side-search-input", keys = { escape = "clear_search" } },
			},
			if state.query ~= "" then ui.row { msg = "clear_search", ui.icon { name = "circle-x", size = "small" } } else nil,
		},
		ui.column { grow = true, scroll = true, gap = "none", table.unpack(children) },
		ui.row { class = "side-footer", msg = { open_url = maps.terms }, self_align = "start", gap = "small", align = "center",
			ui.text { "Terms & Conditions", size = "small" }, ui.icon { name = "chevron-right", size = "small" },
		},
	}
end

------------------------------------------------------------------------
-- Map controls

local function tool(child, msg, tip, options)
	options = options or {}
	return ui.row { width = 40, height = options.height or 40, shrink = false, align = "center", justify = "center", class = if options.disabled then "map-tool map-off" else "map-tool",
		state = if options.on and not options.disabled then "on" else nil,
		msg = if options.disabled then nil else msg, to = options.to, tooltip = tip,
		child,
	}
end

local function hairline() return ui.row { height = 1, shrink = false, class = "map-hairline" } end

local function style_picker(state, maps, app)
	local cards = {}
	local row = {}
	for index, style in maps.styles or {} do
		local chosen = style.id == maps.style
		table.insert(row, ui.column { grow = true, width = 1, class = "style-card", selected = chosen, msg = send("style", { style = style.id }).msg, to = TO, tooltip = style.detail,
			ui.column { height = 54, class = "style-swatch", style = { background = style.swatch[1] }, justify = "center", gap = "none",
				ui.row { height = 4, class = "style-road", style = { background = style.swatch[2], margin = { 0, 6 } } },
				ui.row { height = 10 },
				ui.row { height = 3, class = "style-road", style = { background = style.swatch[2], margin = { 0, 20, 0, 12 } } },
			},
			ui.row { gap = "small", align = "center",
				ui.text { style.title, size = "small", weight = if chosen then "semibold" else "medium", truncate = true },
				if style.needs_key then ui.icon { name = if style.has_key then "lock-open" else "key-round", size = "small", tone = "muted" } else nil,
			},
		})
		if #row == 3 or index == #maps.styles then
			while #row < 3 do table.insert(row, ui.column { grow = true, width = 1 }) end
			table.insert(cards, ui.row { gap = "small", table.unpack(row) })
			row = {}
		end
	end
	local current
	for _, style in maps.styles or {} do
		if style.id == maps.style then current = style end
	end
	return ui.column { class = "styles", width = 340,
		ui.text { "Map Styles", weight = "bold", size = "large" },
		ui.column { gap = "small", table.unpack(cards) },
		ui.column { gap = "small",
			ui.text { "Look Around uses Panoramax. A Mapillary token adds far more streets.", size = "small", tone = "muted" },
			ui.input { id = "key-mapillary", value = state.keys.mapillary or maps.street_token, placeholder = "Mapillary token · Enter", change = { msg = "key_draft", style = "mapillary" }, submit = { msg = "key_save", style = "mapillary" } },
			ui.row { class = "side-link", msg = { open_url = "https://www.mapillary.com/dashboard/developers" }, self_align = "start", ui.text { "Get a token…", size = "small" } },
		},
		if current and current.needs_key then ui.column { gap = "small",
			ui.text { current.title .. " needs an API key from its provider.", size = "small", tone = "muted" },
			ui.input { id = "key-" .. current.id, value = state.keys[current.id] or maps.key, placeholder = "Paste API key · Enter", change = { msg = "key_draft", style = current.id }, submit = { msg = "key_save", style = current.id } },
			ui.row { class = "side-link", msg = { open_url = current.url }, self_align = "start", ui.text { "Get a key…", size = "small" } },
		} else nil,
	}
end

local function avatar_menu(maps, app)
	local function item(text, icon, msg, to)
		return ui.row { class = "menu-row", msg = msg, to = to, align = "center", ui.icon { name = icon, size = "small" }, ui.text { text } }
	end
	return ui.column { width = 250, padding = "small", gap = "none",
		item("Light Appearance", "sun", "light", "app:appearance"),
		item("Dark Appearance", "moon", "dark", "app:appearance"),
		item("Match System", "monitor", "system", "app:appearance"),
		item("Clear Recents", "trash-2", send("clear_recents").msg, TO),
	}
end

local function controls(state, maps, app)
	local dark = app.dark
	local ink = if dark then "#e5e5ea" else "#3c3c43"
	return ui.column { float = { top = 10, right = 10 }, style = { gap = 10 }, align = "end",
		ui.popover { open = state.popup == "avatar", dismiss = "close_popup", side = "left", align = "start", gap = 8,
			content = avatar_menu(maps, app),
			ui.row { width = 40, height = 40, align = "center", justify = "center", class = "map-avatar", msg = { popup = "avatar" }, tooltip = "Moonmap", ui.icon { name = "user", size = "large" } },
		},
		ui.column { class = "map-card", gap = "none",
			ui.popover { open = state.popup == "styles", dismiss = "close_popup", side = "left", align = "start", gap = 8,
				content = style_picker(state, maps, app),
				tool(ui.icon { name = "map", size = "large", style = { color = ink } }, { popup = "styles" }, "Map Styles", { on = state.popup == "styles" }),
			},
			hairline(),
			tool(png("location", 18, if maps.location_status == "located" then "blue" else dark), "locate",
				if maps.accuracy then "Go to My Location · within " .. tostring(maps.accuracy) .. " m · ⌘L" else "Go to My Location · ⌘L", { on = maps.location_status == "located" }),
		},
		ui.column { class = "map-card", gap = "none",
			tool(ui.icon { name = "plus", size = "large", style = { color = ink } }, "zoom_in", "Zoom In · ⌘+", { disabled = seen.zoom >= MAX_ZOOM }),
			hairline(),
			tool(ui.icon { name = "minus", size = "large", style = { color = ink } }, "zoom_out", "Zoom Out · ⌘−", { disabled = seen.zoom <= 2 }),
		},
		ui.column { width = 40, height = 40, align = "center", justify = "center", gap = "none", class = "map-round", tooltip = "North is up",
			ui.text { "N", class = "map-compass-n", style = { color = ink } },
			ui.row { width = 4, height = 4, style = { background = "#ff3b30", radius = 2 } },
		},
		-- Make it yours (Moonframe's `feedback` mod): say what to change, and
		-- a coding agent changes the app in front of the user.
		ui.row { width = 40, height = 40, align = "center", justify = "center", class = "map-round", ui.mount "feedback" },
	}
end

local function weather(maps)
	local now = maps.weather
	if not now then return nil end
	local aqi = now.aqi
	local tone = if aqi == nil then nil elseif aqi <= 50 then "#34c759" elseif aqi <= 100 then "#ffcc00" else "#ff9500"
	return ui.column { float = { bottom = 24, right = 10 }, class = "map-weather", tooltip = now.summary, align = "end",
		ui.row { gap = "small", align = "center", ui.icon { name = now.icon }, ui.text { tostring(math.round(now.celsius)) .. "°", class = "map-weather-temp" } },
		if aqi then ui.row { gap = "small", align = "center", ui.text { "AQI " .. tostring(aqi), class = "map-weather-aqi" }, ui.row { width = 6, height = 6, style = { background = tone, radius = 3 } } } else nil,
	}
end

------------------------------------------------------------------------
-- Place card

-- The place card's inner width, and its action buttons' fixed sizes: a
-- popover anchor (the Guide button) does not share a row's free space, so
-- every button gets an explicit width rather than `grow`.
local SHEET_WIDTH = 360
local HERO_HEIGHT = 200
local SHEET_RIGHT = SHEET_WIDTH + 16
local CARD_INNER = SHEET_WIDTH - 2 * 16
local ACTION_GAP = 6
local ACTION_HEIGHT = 50
local function action_width(per_row)
	return math.floor((CARD_INNER - (per_row - 1) * ACTION_GAP) / per_row)
end

local function action_button(icon, text, msg, to, on, primary, copy, per_row, disabled, tip)
	return ui.column { width = action_width(per_row or 4), height = ACTION_HEIGHT, shrink = false, align = "center", justify = "center",
		class = if primary then "place-primary" elseif disabled then "place-action map-off" else "place-action", state = if on and not disabled then "on" else nil,
		msg = if disabled then nil else msg, to = to, copy = copy, tooltip = tip,
		glyph(icon, 15, if primary then "white" else "blue"),
		ui.text { text, size = "small", weight = "medium", truncate = true },
	}
end

local NO_LOOK = "No 360° imagery here. A Mapillary token in Map Styles adds more streets."

-- Opens the panorama the provider found, if it found any.
local function look_message(maps, place)
	return if maps.look_around then { look_around = true, place = place } else nil
end

local function action_row(...)
	return ui.row { style = { gap = ACTION_GAP }, ... }
end

-- One line of the sheet's Details section.
local function detail(title, value, msg, copy)
	return ui.column { gap = "none", shrink = false, class = if msg or copy then "sheet-detail sheet-link" else "sheet-detail", msg = msg, copy = copy,
		ui.text { title, class = "side-sub" },
		ui.text { value, class = if msg then "sheet-detail-link" else nil, lines = 2 },
	}
end

local function sheet_stat(title, value)
	return ui.column { grow = true, align = "center", gap = "none", shrink = false,
		ui.text { title, class = "side-sub" },
		ui.text { value, weight = "semibold", truncate = true },
	}
end

-- The bar along the sheet's foot: the place's quiet actions.
local function sheet_bar(state, maps, place)
	local guide_items = {}
	for _, guide in maps.guides or {} do
		local inside = table.find(place.guides or {}, guide.id) ~= nil
		table.insert(guide_items, ui.row { class = "menu-row", align = "center", msg = send("guide_toggle", { guide = guide.id, id = place.id }).msg, to = TO,
			ui.icon { name = if inside then "square-check" else "square", size = "small" }, ui.text { guide.name },
		})
	end
	table.insert(guide_items, ui.input { id = "card-guide", value = state.guide_name, placeholder = "New guide · Enter", change = "guide_name", submit = { msg = "guide_new_with", id = place.id } })
	local function button(icon, tip, msg, to, on, copy)
		return ui.row { width = 38, height = 34, align = "center", justify = "center", class = "sheet-tool", state = if on then "on" else nil, msg = msg, to = to, copy = copy, tooltip = tip,
			ui.icon { name = icon },
		}
	end
	return ui.row { float = { bottom = 12, left = 0, right = 0 }, justify = "center",
		ui.row { class = "sheet-bar", align = "center",
			ui.popover { open = state.popup == "guide", dismiss = "close_popup", side = "top", align = "start", width = 230,
				content = ui.column { padding = "small", gap = "none", table.unpack(guide_items) },
				button("plus", "Add to Guide", { popup = "guide" }, nil, #(place.guides or {}) > 0),
			},
			button(if place.saved then "star-fill" else "star", if place.saved then "Saved" else "Save", send("save", { id = place.id }).msg, TO, place.saved),
			button(if place.pinned then "pin-off" else "pin", if place.pinned then "Unpin" else "Pin", send("pin", { id = place.id }).msg, TO, place.pinned),
			button("copy", "Copy Coordinates", nil, nil, false, string.format("%.6f, %.6f", place.lat, place.lon)),
		},
	}
end

local function sheet_directions(state, place)
	local modes = {}
	for _, mode in { "car", "walk", "bike" } do
		table.insert(modes, ui.row { grow = true, justify = "center", class = "place-mode", selected = state.mode == mode, msg = { mode = mode }, tooltip = ({ car = "Drive", walk = "Walk", bike = "Cycle" })[mode], glyph(MODE_ICON[mode], 15, "blue") })
	end
	return {
		ui.row { class = "place-modes", table.unpack(modes) },
		ui.row { align = "end", gap = "small",
			ui.text { if state.eta then state.eta.time else "Finding a route…", class = "place-eta" },
			if state.eta then ui.text { state.eta.distance, class = "side-sub" } else nil,
		},
		action_row(
			action_button("bookmark", "Save Route", send("route_save", { id = place.id, mode = state.mode }).msg, TO, false, false, nil, 2),
			action_button("close", "End", "end_directions", nil, false, false, nil, 2)
		),
	}
end

-- What Maps' place sheet shows, from open data: nothing here gives ratings
-- or offers, so a section with nothing to say is left out.
local function place_sheet(state, maps, app)
	local place = maps.selected
	if not place then return nil end
	local info = place.details or {}
	local photos = info.photos or {}
	local line = {}
	if info.kind and info.kind ~= "" then table.insert(line, info.kind) end
	table.insert(line, if info.area and info.area ~= "" then info.area else place.subtitle)
	-- The same height from the first frame, so nothing below it moves.
	local hero
	if #photos > 0 then
		hero = ui.image { src = photos[1], width = SHEET_WIDTH, height = HERO_HEIGHT, fit = "cover", class = "sheet-hero" }
	elseif place.gathering then
		hero = ui.icon { name = "loader-circle", spin = true, class = "sheet-wait" }
	else
		hero = ui.icon { name = place.icon or "pin", size = "title", style = { color = "#ffffff" } }
	end
	local body = {
		ui.row { height = HERO_HEIGHT, width = SHEET_WIDTH, shrink = false, align = "center", justify = "center",
			class = "sheet-hero", style = if #photos > 0 or place.gathering then nil else { background = place.color or "#8e8e93" },
			hero,
		},
	}
	local content = {
		ui.column { gap = "none", align = "center", shrink = false,
			ui.text { place.name, class = "place-name", lines = 2 },
			ui.text { table.concat(line, " · "), class = "side-sub", lines = 2 },
		},
	}
	if state.directions == place.id then
		for _, item in sheet_directions(state, place) do table.insert(content, item) end
	else
		local actions = { { "png:car", place.drive_time or "Directions", { directions = place.id }, true } }
		-- Held open while they may still come, so the row keeps its shape.
		local phone, website = info.phone and info.phone ~= "", info.website and info.website ~= ""
		if phone or place.gathering then table.insert(actions, { "png:phone", "Call", if phone then { open_url = "tel:" .. string.gsub(info.phone, "%s", "") } else nil, false, not phone }) end
		if website or place.gathering then table.insert(actions, { "globe", "Website", if website then { open_url = info.website } else nil, false, not website }) end
		table.insert(actions, { "eye", "Look Around", look_message(maps, place.id), false, maps.look_around == nil, NO_LOOK })
		local buttons = {}
		for _, action in actions do
			table.insert(buttons, action_button(action[1], action[2], action[3], nil, false, action[4], nil, #actions, action[5], if action[5] then action[6] else nil))
		end
		table.insert(content, action_row(table.unpack(buttons)))
		table.insert(content, ui.row { class = "sheet-stats", shrink = false,
			sheet_stat("Distance", place.distance),
			if info.kind and info.kind ~= "" then sheet_stat("Type", info.kind) elseif place.gathering then sheet_stat("Type", "–") else nil,
		})
		-- Boxes hold the strip open for the pictures still on their way.
		if #photos > 1 or place.gathering then
			local strip = {}
			for index = 2, #photos do
				table.insert(strip, ui.row { width = 150, height = 110, shrink = false, class = "sheet-photo",
					ui.image { src = photos[index], width = 150, height = 110, fit = "cover", class = "sheet-photo" },
				})
			end
			if place.gathering then
				for _ = #strip + 1, 3 do
					table.insert(strip, ui.row { width = 150, height = 110, shrink = false, class = "sheet-photo" })
				end
			end
			table.insert(content, ui.row { scroll = "x", shrink = false, class = "sheet-photos", table.unpack(strip) })
		end
		if place.gathering and (info.about or "") == "" then
			table.insert(content, ui.column { gap = "small", shrink = false,
				ui.text { "About", class = "sheet-heading" },
				loading { skeleton = true, count = 4 },
			})
		elseif info.about and info.about ~= "" then
			table.insert(content, ui.column { gap = "small", shrink = false,
				ui.text { "About", class = "sheet-heading" },
				ui.text { info.about },
				if info.source ~= "" then ui.row { class = "side-link sheet-source", msg = { open_url = info.source }, self_align = "start", ui.text { "From Wikipedia, CC BY-SA", size = "small" } } else nil,
			})
		end
		local rows = {}
		if info.hours and #info.hours > 0 then table.insert(rows, detail("Hours", table.concat(info.hours, "\n"))) end
		if info.phone and info.phone ~= "" then table.insert(rows, detail("Phone", info.phone, { open_url = "tel:" .. string.gsub(info.phone, "%s", "") })) end
		if info.website and info.website ~= "" then table.insert(rows, detail("Website", info.website, { open_url = info.website })) end
		if place.subtitle ~= "" then table.insert(rows, detail("Address", place.subtitle)) end
		table.insert(rows, detail("Coordinates", string.format("%.5f, %.5f", place.lat, place.lon), nil, string.format("%.6f, %.6f", place.lat, place.lon)))
		table.insert(content, ui.column { gap = "small", shrink = false,
			ui.text { "Details", class = "sheet-heading" },
			ui.column { class = "sheet-details", shrink = false, table.unpack(rows) },
		})
		-- Room for the bar at the foot.
		table.insert(content, ui.row { height = 48, shrink = false })
	end
	table.insert(body, ui.column { class = "sheet-body", shrink = false, table.unpack(content) })
	return ui.column { float = { top = 8, bottom = 8, left = 8 }, width = SHEET_WIDTH, class = "sheet",
		ui.column { grow = true, scroll = true, gap = "none", table.unpack(body) },
		ui.row { float = { top = 10, right = 10 }, width = 28, height = 28, align = "center", justify = "center", class = "sheet-close", msg = "close_place", tooltip = "Close", ui.icon { name = "close", size = "small" } },
		if state.directions ~= place.id then sheet_bar(state, maps, place) else nil,
	}
end

------------------------------------------------------------------------
-- The map

local function pins(state, maps)
	local list, seen_ids = {}, {}
	local function add(place, pin)
		if seen_ids[place.id] then return end
		seen_ids[place.id] = true
		pin.lat, pin.lon = place.lat, place.lon
		pin.msg = pin.msg or { action = "open", id = place.id }
		table.insert(list, pin)
	end
	local selected = maps.selected
	if selected then add(selected, { shape = "balloon", icon = selected.icon, color = selected.color, selected = true }) end
	local showing = {}
	if state.query ~= "" then showing = maps.results or {}
	elseif state.view == "saved" then showing = maps.saved or {}
	elseif state.view == "guide" then showing = (find_guide(maps, state.guide) or { places = {} }).places
	elseif state.view == "visited" then showing = maps.visited or {}
	elseif state.view == "added" then showing = maps.added or {} end
	for _, place in showing do add(place, { shape = "disc", icon = place.icon, color = place.color }) end
	for _, place in maps.pinned or {} do add(place, { shape = "balloon", icon = "pin", color = "#f0424e" }) end
	local here = maps.location
	-- Maps' blue dot, breathing while the fix is live; grey when it is
	-- only where the user was last seen.
	if here then
		local live = maps.location_status == "located"
		table.insert(list, { lat = here.lat, lon = here.lon, color = if live then "#0a84ff" else "#8e8e93", selected = true, pulse = live, msg = "locate" })
	end
	return list
end

local function routes(state, maps)
	local place, here = maps.selected, maps.location
	if not (place and here and state.directions == place.id) then return {} end
	return { { via = { here.lat, here.lon, place.lat, place.lon }, mode = state.mode, color = "#0a84ff", stroke = 6 } }
end

-- Why the dot is grey, when the location service said why.
local function location_notice(maps)
	local text = if maps.location_status == "denied" then "Location access is off. Turn it on in System Settings › Privacy & Security › Location Services."
		elseif maps.location_status == "unbundled" then "Your location needs Moonmap running as an app: start it with `just run` or `just app`."
		elseif maps.location_status == "approximate" then "Precise Location is off: your position is approximate. Turn it on in System Settings › Privacy & Security › Location Services."
		else nil
	if not text then return nil end
	return ui.row { float = { bottom = 12, left = (if maps.selected then SHEET_RIGHT else 12) + 52 }, max_width = 520, class = "map-banner", align = "center",
		ui.icon { name = "triangle-alert", tone = "warning" },
		ui.text { text, size = "small" },
	}
end

-- A 360° panorama over the map: `street-view` draws it and takes the drags.
local function look_around(state, maps)
	return ui.column { float = { top = 0, bottom = 0, left = 0, right = 0 }, gap = "none", class = "map-look",
		ui.row { height = 52, shrink = false, align = "center", drag = true, class = "map-look-bar",
			style = { padding = { 0, 14, 0, if maps.sidebar == false then 84 else 14 } },
			ui.text { "Look Around", weight = "semibold" },
			ui.spacer(),
			ui.row { class = "place-close", msg = "close_look", tooltip = "Close Look Around", ui.icon { name = "close", size = "small" } },
		},
		ui.column { grow = true, ui.view { name = "street-view", id = "street", props = state.look } },
	}
end

local function map_area(state, maps, app)
	-- No center yet: the map would open on 0, 0 and then jump.
	if not maps.ready then
		return ui.column { grow = true, class = "map-loading", loading { block = true, text = "Finding your location…" } }
	end
	local style = maps.style or "esri-streets"
	local key = maps.key or ""
	local keyless = false
	for _, entry in maps.styles or {} do
		if entry.id == style and entry.needs_key and key == "" then keyless = true end
	end
	-- A launch opens where the user last was.
	local start = if maps.located_before then maps.location else maps.center
	local center = state.center or (if start then { start.lat, start.lon } else nil)
	local zoom = state.zoom or CLOSE_ZOOM
	if center then seen.lat, seen.lon = center[1], center[2] end
	seen.zoom = zoom
	return ui.column { grow = true, class = "map-area",
		ui.column { float = { top = 0, bottom = 0, left = -OVERHANG, right = -OVERHANG },
			ui.map {
				grow = true,
				provider = if keyless then "esri-streets" else style :: any,
				key = if key ~= "" then key else nil,
				center = center,
				glide = true,
				zoom = zoom,
				pins = pins(state, maps),
				routes = routes(state, maps),
				move = { action = "moved" },
				routed = { msg = "routed" },
			},
		},
		-- Under the sheet, so what it takes never reaches the map.
		if maps.selected then ui.column { float = { top = 8, bottom = 8, left = 8 }, width = SHEET_WIDTH, ui.view { name = "shield", id = "sheet" } } else nil,
		place_sheet(state, maps, app),
		if maps.sidebar == false then ui.row { float = { top = 10, left = 84 }, class = "map-card", gap = "none",
			tool(ui.icon { name = "panel-left", size = "large" }, send("sidebar", { open = true }).msg, "Show Sidebar · ⌃⌘S", { to = TO }),
		} else nil,
		if keyless then ui.row { float = { top = 12, left = if maps.selected then SHEET_RIGHT else 12 }, class = "map-banner", align = "center",
			ui.icon { name = "key-round", tone = "warning" },
			ui.text { "This style needs an API key; showing Explore until one is set.", size = "small" },
		} else nil,
		location_notice(maps),
		controls(state, maps, app),
		ui.row { float = { bottom = 12, left = if maps.selected then SHEET_RIGHT else 12 }, width = 40, height = 40, align = "center", justify = "center", class = if maps.look_around then "map-round map-tool" else "map-round map-tool map-off",
			msg = look_message(maps, maps.selected and maps.selected.id), tooltip = if maps.look_around then "Look Around" else NO_LOOK,
			png("binoculars", 20, app.dark),
		},
		weather(maps),
		ui.row { float = "bottom-right", class = "map-credit", ui.text { maps.attribution or "", size = "small" } },
		-- Choosing another place returns to the map.
		if state.look and state.look.place == (maps.selected and maps.selected.id) then look_around(state, maps) else nil,
	}
end

------------------------------------------------------------------------

return mod {
	window = { titlebar = "hidden", traffic_lights = { x = 18, y = 20 }, size = { width = 1440, height = 900 } },
	about = { version = "0.1", description = "Maps on Moonframe, with the tiles you choose." },
	menus = {
		{ title = "View", items = {
			{ label = "Toggle Sidebar", msg = "toggle_sidebar", shortcut = "ctrl-secondary-s" },
			{ label = "Zoom In", msg = "zoom_in", shortcut = "secondary-=" },
			{ label = "Zoom Out", msg = "zoom_out", shortcut = "secondary--" },
			{ label = "Go to My Location", msg = "locate", shortcut = "secondary-l" },
			{ label = "Map Styles…", msg = { popup = "styles" }, shortcut = "secondary-shift-m" },
		} },
		{ title = "Edit", items = {
			{ label = "Find", msg = "focus_search", shortcut = "secondary-f" },
		} },
	},
	subscribe = { "provider/maps" },
	init = function(previous)
		return {
			query = "", view = "home", guide = nil, guide_name = "", popup = nil, focus = false,
			center = nil, zoom = nil, nudge = false, directions = nil, mode = "car", eta = nil,
			keys = {}, maps = if previous then previous.maps else {},
		}
	end,
	update = function(state, msg)
		local maps = state.maps or {}
		if type(msg) == "string" then
			if msg == "zoom_in" then state.center = { seen.lat, seen.lon }; state.zoom = math.min(MAX_ZOOM, seen.zoom + 1)
			elseif msg == "zoom_out" then state.center = { seen.lat, seen.lon }; state.zoom = math.max(2, seen.zoom - 1)
			elseif msg == "locate" then
				if maps.location then fly(state, maps.location.lat, maps.location.lon, math.max(seen.zoom, CLOSE_ZOOM)) end
			elseif msg == "close_popup" then state.popup = nil
			elseif msg == "close_look" then state.look = nil
			elseif msg == "focus_search" then state.focus = not state.focus; state.view = "home"
			elseif msg == "clear_search" then state.query = ""; return state, { send("clear_search") }
			elseif msg == "close_place" then state.directions = nil; state.eta = nil; return state, { send("close") }
			elseif msg == "end_directions" then state.directions = nil; state.eta = nil
			elseif msg == "toggle_sidebar" then return state, { send("sidebar") }
			elseif msg == "guide_new" then
				local name = state.guide_name; state.guide_name = ""
				return state, { send("guide_new", { name = name }) }
			end
			return
		end
		if type(msg) ~= "table" then return end
		if msg.topic == "provider/maps" then
			local before = state.maps and state.maps.selected
			state.maps = msg.value or {}
			-- The first live fix: open on the user, as Maps does.
			if state.maps.location_status == "located" and not state.found and state.maps.location then
				state.found = true
				fly(state, state.maps.location.lat, state.maps.location.lon, math.max(seen.zoom, CLOSE_ZOOM))
			end
			local after = state.maps.selected
			-- A newly opened place: fly to it.
			if after and (not before or before.id ~= after.id) then
				fly(state, after.lat, after.lon, math.max(seen.zoom, CLOSE_ZOOM))
				if state.directions ~= after.id then state.directions = nil; state.eta = nil end
				if state.pending_directions == after.id then
					state.pending_directions = nil
					state.directions = after.id
					local frame = state.maps.location and fit({ after, state.maps.location }, 700, 600)
					if frame then fly(state, frame.lat, frame.lon, frame.zoom) end
				end
			end
			-- New results: frame them.
			local results = state.maps.results or {}
			if #results > 0 and state.framed ~= state.maps.query then
				state.framed = state.maps.query
				local frame = fit(results, 800, 700)
				if frame then fly(state, frame.lat, frame.lon, math.min(frame.zoom, CLOSE_ZOOM)) end
			end
			return
		end
		if msg.action == "moved" then
			state.center = { msg.lat, msg.lon }
			state.zoom = msg.zoom
			seen.lat, seen.lon, seen.zoom = msg.lat, msg.lon, msg.zoom
			return state, { send("moved", { lat = msg.lat, lon = msg.lon, zoom = msg.zoom }) }
		elseif msg.action then
			return state, { { to = TO, msg = msg } }
		elseif msg.msg == "query" then
			state.query = msg.value or ""
			state.view = "home"
			if msg.submit then return state, { send("search", { query = state.query }) } end
			if state.query == "" then return state, { send("clear_search") } end
		elseif msg.msg == "search" then
			state.query = msg.value or state.query
			state.view = "home"
			return state, { send("search", { query = state.query }) }
		elseif msg.msg == "guide_name" then state.guide_name = msg.value or ""
		elseif msg.msg == "guide_new_with" then
			local name = state.guide_name; state.guide_name = ""; state.popup = nil
			return state, { send("guide_new", { name = name, id = msg.id }) }
		elseif msg.msg == "key_draft" then state.keys[msg.style] = msg.value
		elseif msg.msg == "key_save" then
			state.keys[msg.style] = nil
			return state, { send("key", { style = msg.style, key = msg.value }) }
		elseif msg.msg == "routed" then
			local function time(seconds)
				local minutes = math.max(1, math.round(seconds / 60))
				if minutes < 60 then return tostring(minutes) .. " min" end
				local hours, rest = minutes // 60, minutes % 60
				return tostring(hours) .. " hr" .. (if rest > 0 then " " .. tostring(rest) .. " min" else "")
			end
			local meters = msg.meters or 0
			state.eta = { time = time(msg.seconds or 0), distance = if meters < 1000 then tostring(math.round(meters / 10) * 10) .. " m" else string.format("%.1f km", meters / 1000) }
		elseif msg.view then
			state.view = msg.view
			state.guide = msg.guide
			state.query = ""
		elseif msg.delete_guide then
			state.view = "guides"
			return state, { send("guide_remove", { guide = msg.delete_guide }) }
		elseif msg.popup then state.popup = if state.popup == msg.popup then nil else msg.popup
		elseif msg.directions then
			-- From a suggestion or a saved route: open the place, then route.
			if msg.mode then state.mode = msg.mode end
			state.eta = nil
			if msg.pick then
				if maps.selected and maps.selected.id == msg.pick then state.directions = msg.pick
				else state.pending_directions = msg.pick end
				return state, { send("open", { id = msg.pick }) }
			end
			state.directions = msg.directions
			if maps.selected and maps.location then
				local frame = fit({ maps.selected, maps.location }, 700, 600)
				if frame then fly(state, frame.lat, frame.lon, frame.zoom) end
			end
		elseif msg.mode then state.mode = msg.mode; state.eta = nil
		elseif msg.open_url then return state, { { to = "app:open", msg = msg.open_url } }
		elseif msg.look_around then
			local shot, chosen = maps.look_around, maps.selected
			if shot then
				state.look = table.clone(shot)
				state.look.place = msg.place
				state.look.token = maps.street_token
				state.look.toward_lat = chosen and chosen.lat
				state.look.toward_lon = chosen and chosen.lon
			end
		end
	end,
	view = function(state, app)
		local maps = app.maps or {}
		-- The sidebar is a pane so hiding it slides it away (the
		-- `.side-pane` transition) while the map widens into its room.
		return ui.hsplit { id = "sidebar-and-map", divider = false,
			ui.pane { size = SIDEBAR, fixed = true, visible = maps.sidebar ~= false, class = "side-pane",
				sidebar(state, maps, app),
			},
			ui.pane { map_area(state, maps, app) },
		}
	end,
}

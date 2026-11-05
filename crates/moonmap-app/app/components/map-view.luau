-- The map, apart from the shell. Moonframe runs the view of whoever hears a
-- message, and `ui.map` tells where it looks on every step of a gesture: here
-- that runs this short view, not the sidebar, the sheet and the controls.
--
-- This instance alone knows where the map is. Every view hands that back, so
-- that a rebuild changing the pins is no order to go back: Moonframe takes
-- only what it told as an echo. The shell orders moves with `fly` and sends
-- `zoom_in` and `zoom_out`.

type Props = {
	id: string?,
	provider: any,
	key: string?,
	start: { number }?,
	fly: { id: number, center: { number }, zoom: number? }?,
	pins: { any },
	routes: { any },
	close: number,
	least: number,
	most: number,
	edge: string?,
}

local TO = "provider:maps"

-- Where the map is, once it has been moved or ordered. A local, so that a
-- move changes nothing the app saves.
local seen: { lat: number, lon: number, zoom: number }? = nil
-- The last `fly` obeyed.
local flown = 0

local function edge(zoom: number, props: Props): string?
	return if zoom >= props.most then "in" elseif zoom <= props.least then "out" else nil
end

-- The sends a change of `seen` needs: the shell greys a zoom button at the
-- edge, and hears only of crossing one.
local function crossing(props: Props, sends: { any }): { any }
	local now = if seen then edge(seen.zoom, props) else nil
	if now ~= props.edge then table.insert(sends, { to = "parent", msg = { msg = "zoom_edge", edge = now } }) end
	return sends
end

-- Where the view draws the map from, before any move: the launch position.
local function here(props: Props): { lat: number, lon: number, zoom: number }
	local start = props.start
	return seen or { lat = if start then start[1] else 0, lon = if start then start[2] else 0, zoom = props.close }
end

return component {
	title = "Moonmap's map",
	category = "Moonmap",
	description = "`ui.map` with its position kept out of the shell.",
	props = {
		["provider"] = "the tile provider",
		["key"] = "its API key, if it needs one",
		["start"] = "{ lat, lon } to show until the map is moved or ordered",
		["fly"] = "the latest order: `id` grows, `center` is { lat, lon }, and without `zoom` the map keeps its own, no farther than `close`",
		["pins"] = "`ui.map`'s pins",
		["routes"] = "`ui.map`'s routes",
		["close"] = "the zoom a place is shown at",
		["least"] = "the widest zoom",
		["most"] = "the deepest zoom",
		["edge"] = "\"in\" or \"out\" when the shell thinks the zoom is at that end",
	},

	example = { provider = "esri-streets", start = { 37.788, -122.4075 }, pins = {}, routes = {}, close = 15.8, least = 2, most = 19 },

	init = function() return {} end,

	update = function(state, msg, props: Props)
		if msg == "zoom_in" or msg == "zoom_out" then
			local at = here(props)
			local zoom = at.zoom + (if msg == "zoom_in" then 1 else -1)
			seen = { lat = at.lat, lon = at.lon, zoom = math.clamp(zoom, props.least, props.most) }
			return state, crossing(props, {})
		end
		if type(msg) == "table" and msg.action == "moved" then
			seen = { lat = msg.lat, lon = msg.lon, zoom = msg.zoom }
			return state, crossing(props, { { to = TO, msg = msg } })
		end
		-- Pins and routes speak to the shell.
		return state, { { to = "parent", msg = msg } }
	end,

	build = function(props: Props)
		local fly = props.fly
		if fly and fly.id ~= flown then
			flown = fly.id
			local zoom = fly.zoom or math.max(here(props).zoom, props.close)
			seen = { lat = fly.center[1], lon = fly.center[2], zoom = math.clamp(zoom, props.least, props.most) }
		end
		local at = here(props)
		return ui.map {
			grow = true,
			provider = props.provider,
			key = props.key,
			-- No position yet: the map frames its pins.
			center = if seen or props.start then { at.lat, at.lon } else nil,
			glide = true,
			zoom = at.zoom,
			pins = props.pins,
			routes = props.routes,
			move = { action = "moved" },
			routed = { msg = "routed" },
		}
	end,
}

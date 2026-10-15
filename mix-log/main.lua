-- mix-log: a OpenFadeR plugin in Lua (protocol and the `rk` API: docs/plugins.md).
--
-- `track_played` (the track stayed on air as long as the play history needs) appends one line to today's log;
-- `deck_play` remembers which deck played what, so the line says the deck. The actions open the log or count its
-- tracks. Settings (plugin manager): folder, csv / txt, whether BPM and key are written, a toast per logged track.

local M = {}

local function folder(settings)
  if settings.folder and settings.folder ~= "" then
    return settings.folder
  end
  return rk.data_dir
end

local function log_file(settings)
  return folder(settings) .. "/mix-" .. os.date("%Y-%m-%d") .. "." .. (settings.format or "csv")
end

-- Which deck each file was last played on (`deck_play`), kept between requests in a small JSON file.
local function decks_file()
  return rk.data_dir .. "/decks.json"
end

local function remember(track, deck)
  local known = {}
  local text = rk.fs.read(decks_file())
  if text then known = rk.json.decode(text) end
  known[track.file or track.title or "?"] = deck
  rk.fs.write(decks_file(), rk.json.encode(known))
end

local function deck_of(track)
  local text = rk.fs.read(decks_file())
  if not text then return "" end
  return tostring(rk.json.decode(text)[track.file or track.title or "?"] or "")
end

local function csv(v)
  v = tostring(v or "")
  if v:find('[,"\n]') then v = '"' .. v:gsub('"', '""') .. '"' end
  return v
end

local function line(settings, track)
  local time, deck = os.date("%H:%M:%S"), deck_of(track)
  local bpm = track.bpm and track.bpm > 0 and string.format("%.2f", track.bpm) or ""
  if settings.format == "txt" then
    local artist = track.artist or ""
    local s = time .. "  " .. (artist ~= "" and (artist .. " - ") or "") .. (track.title or "")
    if settings.details and (bpm ~= "" or (track.key or "") ~= "") then
      s = s .. "  [" .. bpm .. " BPM, " .. (track.key or "") .. "]"
    end
    return s .. (deck ~= "" and ("  (deck " .. deck .. ")") or "")
  end
  local cells = {time, deck, csv(track.artist), csv(track.title)}
  if settings.details then
    table.insert(cells, bpm)
    table.insert(cells, csv(track.key))
  end
  return table.concat(cells, ",")
end

local function lines_today(settings)
  local text = rk.fs.read(log_file(settings)) or ""
  local n = 0
  for l in text:gmatch("[^\n]+") do
    if not l:match("^time,") then n = n + 1 end
  end
  return n
end

function M.handle(req)
  local s = req.settings or {}
  if req.op == "event" and req.event == "deck_play" and req.data.track then
    remember(req.data.track, req.data.deck)
    return {}
  end
  if req.op == "event" and req.event == "track_played" then
    local file = log_file(s)
    rk.fs.mkdir(folder(s))
    local text = rk.fs.read(file)
    if not text and (s.format or "csv") == "csv" then
      text = "time,deck,artist,title" .. (s.details and ",bpm,key" or "") .. "\n"
    end
    rk.fs.write(file, (text or "") .. line(s, req.data.track) .. "\n")
    if s.announce then
      return {commands = {{cmd = "toast", text = "mix-log: " .. (req.data.track.title or "track") .. " logged"}}}
    end
    return {}
  end
  if req.op == "action" and req.action == "open" then
    local file = log_file(s)
    if not rk.fs.exists(file) then
      return {commands = {{cmd = "toast", text = "mix-log: nothing played today yet"}}}
    end
    return {commands = {{cmd = "open", url = file}}}
  end
  if req.op == "action" and req.action == "count" then
    local n = lines_today(s)
    return {commands = {{cmd = "toast", text = "mix-log: " .. n .. " track" .. (n == 1 and "" or "s") .. " today"}}}
  end
  return {}
end

return M

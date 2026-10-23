// The script API of Mixxx controller mappings (as the Mixxx manual documents it) on top of the natives of js.rs (`__get`, `__set`…).
// Our own implementation. The natives take and give numbers and text only; callbacks live here.
var __conns = {}, __connId = 0, __timers = {}, __scr = {}, __bindings = [];

function __report(where, e) {
    __print(where + ": " + (e && e.stack ? e.stack : e));
}

var engine = {
    getValue: function (g, k) { return __get(g, k, false); },
    setValue: function (g, k, v) { __set(g, k, +v, false); },
    getParameter: function (g, k) { return __get(g, k, true); },
    setParameter: function (g, k, v) { __set(g, k, +v, true); },
    getParameterForValue: function (g, k, v) { return __param(g, k, +v); },
    makeConnection: function (g, k, cb) {
        var id = ++__connId;
        __conns[id] = { group: g, key: k, cb: cb };
        __watch(g, k, true);
        return {
            id: id,
            isConnected: true,
            trigger: function () { engine.trigger(g, k); },
            disconnect: function () {
                if (__conns[id]) { delete __conns[id]; __watch(g, k, false); this.isConnected = false; }
            }
        };
    },
    connectControl: function (g, k, cb, disconnect) {
        var c = engine.makeConnection(g, k, typeof cb === "string" ? function (v, g2, k2) { (0, eval)(cb)(v, g2, k2); } : cb);
        if (disconnect) { c.disconnect(); }
        return c;
    },
    trigger: function (g, k) {
        var v = __get(g, k, false);
        for (var id in __conns) {
            var c = __conns[id];
            if (c.group === g && c.key === k) { __fire(c, v); }
        }
    },
    beginTimer: function (ms, cb, once) {
        var id = __timer(+ms, once === true);
        if (id) { __timers[id] = { cb: cb, once: once === true }; }
        return id;
    },
    stopTimer: function (id) {
        if (__timers[id]) { delete __timers[id]; __cancel(id); }
    },
    softTakeover: function (g, k, on) { __soft(g, k, !!on); },
    // shortcut: the host has no "ignore the next value"; the next value after a jump is held by soft takeover itself.
    softTakeoverIgnoreNextValue: function () { },
    scratchEnable: function (deck) { __scr[deck] = true; __touch(deck, true); },
    scratchDisable: function (deck) { __scr[deck] = false; __touch(deck, false); },
    scratchTick: function (deck, ticks) { __jog(deck, +ticks); },
    scratchTickV2: function (deck, ticks) { __jog(deck, +ticks); },
    isScratching: function (deck) { return !!__scr[deck]; },
    // shortcut: our brake is one action for stop and spin back; the factor and rate are the user's PREFERENCES.
    brake: function (deck, on) { if (on) { __brake(deck); } },
    spinback: function (deck, on) { if (on) { __brake(deck); } },
    softStart: function (deck, on) { __set("[Channel" + deck + "]", "play", on ? 1 : 0, false); },
    log: function (s) { __print(String(s)); },
    version: function () { return "2.4.0"; }
};

function __fire(c, v) {
    try {
        c.cb(v, c.group, c.key);
    } catch (e) { __report("connection " + c.group + " " + c.key, e); }
}

// Called by the plugin
function __changed(g, k, v) {
    for (var id in __conns) {
        var c = __conns[id];
        if (c.group === g && c.key === k) { __fire(c, v); }
    }
}

function __tick(id) {
    var t = __timers[id];
    if (!t) { return; }
    if (t.once) { delete __timers[id]; }
    try {
        if (typeof t.cb === "function") { t.cb(); } else { (0, eval)(t.cb); }
    } catch (e) { __report("timer", e); }
}

function __bind(i, key) {
    __bindings[i] = (0, eval)("(function (a, b, c, d, e) { return " + key + "(a, b, c, d, e); })");
}

function __in(i, channel, control, value, status, group) {
    try { __bindings[i](channel, control, value, status, group); } catch (e) { __report("script " + i, e); }
}

function __sysex(i, bytes) {
    var data = bytes.split(",").map(Number);
    try { __bindings[i](data, data.length); } catch (e) { __report("sysex " + i, e); }
}

function __incoming(prefix, bytes) {
    var data = bytes.split(",").map(Number);
    __call(prefix, "incomingData", data, data.length);
}

function __call(prefix, name) {
    var args = Array.prototype.slice.call(arguments, 2);
    try {
        // the function's parent is its `this`, as for the mapping's own calls
        var parent = prefix ? (0, eval)("typeof " + prefix + " !== 'undefined' ? " + prefix + " : undefined") : undefined;
        if (parent && typeof parent[name] === "function") { parent[name].apply(parent, args); return true; }
    } catch (e) { __report(prefix + "." + name, e); }
    return false;
}

var midi = {
    sendShortMsg: function (status, d1, d2) { __midi(status, d1, d2); },
    sendSysexMsg: function (data, length) { __sysexOut(Array.prototype.slice.call(data, 0, length || data.length).join(",")); }
};
function print(s) { __print(String(s)); }
var console = {
    log: print, info: print, debug: print,
    warn: function (s) { print("warning: " + s); },
    error: function (s) { print("error: " + s); }
};

var script = {
    samplerRegEx: /^\[Sampler(\d+)\]$/,
    channelRegEx: /^\[Channel(\d+)\]$/,
    eqRegEx: /^\[EqualizerRack1_\[(.*)\]_Effect1\]$/,
    quickEffectRegEx: /^\[QuickEffectRack1_\[(.*)\]\]$/,
    effectUnitRegEx: /^\[EffectRack1_EffectUnit(\d+)\]$/,
    deckFromGroup: function (group) {
        var m = /^\[Channel(\d+)\]$/.exec(group);
        return m ? +m[1] : 0;
    },
    midiDebug: function (channel, control, value, status, group) {
        print("MIDI status 0x" + status.toString(16) + ", ctrl 0x" + control.toString(16) + ", val 0x" + value.toString(16) + ", grp " + group);
    },
    toggleControl: function (g, k) { engine.setValue(g, k, !engine.getValue(g, k)); },
    triggerControl: function (g, k, delay) {
        engine.setValue(g, k, 1);
        engine.beginTimer(delay || 200, function () { engine.setValue(g, k, 0); }, true);
    },
    absoluteLin: function (value, low, high, min, max) {
        min = min || 0; max = max || 127;
        if (value <= min) { return low; }
        if (value >= max) { return high; }
        return low + (value - min) / (max - min) * (high - low);
    },
    absoluteLinInverse: function (value, low, high, min, max) {
        min = min || 0; max = max || 127;
        var r = (value - low) / (high - low);
        return Math.max(min, Math.min(max, min + r * (max - min)));
    },
    absoluteNonLin: function (value, low, mid, high, min, max) {
        min = min || 0; max = max || 127;
        var center = (max - min) / 2;
        if (value <= min) { return low; }
        if (value >= max) { return high; }
        if (value === center) { return mid; }
        if (value < center) { return low + (value - min) / (center - min) * (mid - low); }
        return mid + (value - center) / (max - center) * (high - mid);
    },
    absoluteNonLinInverse: function (value, low, mid, high, min, max) {
        min = min || 0; max = max || 127;
        var center = (max - min) / 2;
        var r = value <= mid ? (value - low) / (mid - low) * (center - min) + min
            : (value - mid) / (high - mid) * (max - center) + center;
        return Math.max(min, Math.min(max, r));
    },
    midiPitch: function (lsb, msb) { return (((msb << 7) | lsb) - 8192) / 8192; },
    spinback: function (channel, control, value, status, group) { engine.spinback(script.deckFromGroup(group), value > 0); },
    brake: function (channel, control, value, status, group) { engine.brake(script.deckFromGroup(group), value > 0); },
    softStart: function (channel, control, value, status, group) { engine.softStart(script.deckFromGroup(group), value > 0); }
};

var bpm = {
    tapButton: function (deck) { engine.setValue("[Channel" + deck + "]", "bpm_tap", 1); }
};

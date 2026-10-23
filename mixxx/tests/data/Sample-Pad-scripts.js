// Scripts of the sample mapping (written for this plugin's tests).
var Sample = {};

Sample.init = function (id, debugging) {
    Sample.id = id;
    Sample.light = engine.makeConnection("[Channel1]", "play_indicator", function (value) {
        midi.sendShortMsg(0x90, 0x40, value ? 0x7f : 0x00);
    });
    engine.softTakeover("[Master]", "crossfader", true);
    engine.beginTimer(100, function () { midi.sendShortMsg(0x90, 0x42, 1); }, true);
    midi.sendSysexMsg([0xF0, 0x7E, 0x01, 0xF7], 4);
};

// jog wheel: centred on 64
Sample.jog = function (channel, control, value, status, group) {
    engine.scratchTick(script.deckFromGroup(group), value - 64);
};

Sample.touch = function (channel, control, value, status, group) {
    var deck = script.deckFromGroup(group);
    if (status === 0x90 && value > 0) {
        engine.scratchEnable(deck, 128, 33 + 1 / 3, 1 / 8, (1 / 8) / 32);
    } else {
        engine.scratchDisable(deck);
    }
};

Sample.shutdown = function () {
    midi.sendShortMsg(0x90, 0x40, 0x00);
};

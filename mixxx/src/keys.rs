//! Mixxx's documented control keys (`[Channel1]`, `play`) as OpenFadeR controls (the control table of `docs/plugins.md`, Controller
//! plugins). A key with no entry here is not translated: reading it answers 0, setting it does nothing.

/// How the host's control takes a value.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// A knob or fader, 0 to 1.
    Level,
    /// A button that follows the value: pressed above 0, released at 0.
    Button,
    /// A button that toggles a state: setting a value presses it only when the state differs.
    Toggle,
    /// An encoder or jog: signed ticks.
    Ticks,
    /// A value to read; setting it does nothing.
    Read,
}

/// Mixxx's value of a control against the host's 0 to 1 parameter.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Conv {
    Id,
    /// -1 to 1 (crossfader, headphone mix, rate).
    Bipolar,
    /// 0 to 4 with 1 in the middle of the knob (EQ and gain).
    Gain,
}

impl Conv {
    pub fn value(self, p: f64) -> f64 {
        match self {
            Conv::Id => p,
            Conv::Bipolar => p * 2.0 - 1.0,
            Conv::Gain if p <= 0.5 => p * 2.0,
            Conv::Gain => 1.0 + (p - 0.5) * 6.0,
        }
    }

    pub fn param(self, v: f64) -> f64 {
        match self {
            Conv::Id => v,
            Conv::Bipolar => (v + 1.0) / 2.0,
            Conv::Gain if v <= 1.0 => v / 2.0,
            Conv::Gain => 0.5 + (v - 1.0) / 6.0,
        }
        .clamp(0.0, 1.0)
    }
}

/// The host controls behind one Mixxx key.
#[derive(Clone, PartialEq, Debug)]
pub struct Spec {
    /// The key to read (and watch).
    pub read: String,
    /// The key to set.
    pub write: String,
    pub kind: Kind,
    pub conv: Conv,
}

fn spec(read: &str, write: &str, kind: Kind, conv: Conv) -> Option<Spec> {
    Some(Spec {
        read: read.to_owned(),
        write: write.to_owned(),
        kind,
        conv,
    })
}

/// `[Channel3]` → 3.
fn deck(group: &str) -> Option<u32> {
    group
        .strip_prefix("[Channel")?
        .strip_suffix(']')?
        .parse()
        .ok()
        .filter(|d| (1..=4).contains(d))
}

/// `[EqualizerRack1_[Channel2]_Effect1]` with `pre` = `[EqualizerRack1_` and `post` = `_Effect1]` → 2.
fn nested_deck(group: &str, pre: &str, post: &str) -> Option<u32> {
    deck(group.strip_prefix(pre)?.strip_suffix(post)?)
}

fn on_deck(d: u32, key: &str) -> Option<Spec> {
    let k = |n: &str| format!("{n}/{d}");
    let button = |n: &str| spec(&k(n), &k(n), Kind::Button, Conv::Id);
    let toggle = |r: &str, w: &str| spec(&k(r), &k(w), Kind::Toggle, Conv::Id);
    let read = |n: &str| spec(&k(n), &k(n), Kind::Read, Conv::Id);
    let level = |r: &str, w: &str, c| spec(&k(r), &k(w), Kind::Level, c);
    let ticks = |n: &str| spec(&k(n), &k(n), Kind::Ticks, Conv::Id);
    match key {
        "play" => toggle("playing", "play"),
        "play_indicator" => read("playing"),
        "cue_default" | "cue_gotoandplay" | "cue_gotoandstop" | "cue_preview" => button("cue"),
        "cue_indicator" => read("at-cue"),
        "sync_enabled" => toggle("synced", "sync"),
        "beatsync" => button("sync"),
        "keylock" => toggle("key-lock", "key-lock"),
        "quantize" => toggle("quantize", "quantize"),
        "slip_enabled" => toggle("slip", "slip"),
        "reverse" => toggle("reverse", "reverse"),
        "pfl" => toggle("cue-monitor", "cue-monitor"),
        "loop_enabled" => read("loop-active"),
        "reloop_toggle" | "reloop_andstop" => button("reloop"),
        "loop_in" => button("loop-in"),
        "loop_out" => button("loop-out"),
        "loop_halve" => button("loop-halve"),
        "loop_double" => button("loop-double"),
        "LoadSelectedTrack" | "LoadSelectedTrackAndPlay" => button("load"),
        "eject" => button("hw-pad-unload"),
        "bpm_tap" => button("grid-tap"),
        "track_loaded" => read("loaded"),
        "bpm" => read("bpm"),
        "playposition" => level("position", "position", Conv::Id),
        "rate" => level("tempo", "tempo", Conv::Bipolar),
        "volume" => level("fader", "fader", Conv::Id),
        "pregain" => level("trim", "trim", Conv::Gain),
        "filterHigh" => level("eq-hi", "eq-hi", Conv::Gain),
        "filterMid" => level("eq-mid", "eq-mid", Conv::Gain),
        "filterLow" => level("eq-low", "eq-low", Conv::Gain),
        "jog" | "wheel" => ticks("jog-bend"),
        "scratch2" => ticks("jog"),
        _ => numbered(d, key),
    }
}

/// `hotcue_3_activate`, `beatloop_4_toggle`, `beatjump_1_forward`.
fn numbered(d: u32, key: &str) -> Option<Spec> {
    let mut parts = key.split('_');
    let (family, n, what) = (parts.next()?, parts.next()?, parts.next()?);
    let n: u32 = n.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let k = |name: String| format!("{name}/{d}");
    match (family, what) {
        ("hotcue", "activate" | "gotoandplay" | "gotoandstop" | "goto") if (1..=8).contains(&n) => {
            let pad = k(format!("pad{n}"));
            spec(&pad, &pad, Kind::Button, Conv::Id)
        }
        ("hotcue", "enabled" | "status") if (1..=8).contains(&n) => {
            let pad = k(format!("pad{n}"));
            spec(&pad, &pad, Kind::Read, Conv::Id)
        }
        ("hotcue", "clear") if (1..=8).contains(&n) => {
            let key = format!("hw-pad-delete/{d}/{}", n - 1);
            spec(&key, &key, Kind::Button, Conv::Id)
        }
        ("beatloop", "toggle" | "activate") if matches!(n, 1 | 2 | 4 | 8 | 16) => {
            let key = k(format!("loop-{n}"));
            spec(&key, &key, Kind::Button, Conv::Id)
        }
        ("beatjump", "forward" | "backward") if matches!(n, 1 | 4) => {
            let dir = if what == "forward" { "fwd" } else { "back" };
            let key = k(format!("jump-{dir}-{n}"));
            spec(&key, &key, Kind::Button, Conv::Id)
        }
        _ => None,
    }
}

fn master(key: &str) -> Option<Spec> {
    match key {
        "crossfader" => spec("crossfader", "crossfader", Kind::Level, Conv::Bipolar),
        "gain" => spec("master", "master", Kind::Level, Conv::Id),
        "headGain" => spec("hp-level", "hp-level", Kind::Level, Conv::Id),
        "headMix" => spec("hp-mix", "hp-mix", Kind::Level, Conv::Bipolar),
        _ => None,
    }
}

fn library(key: &str) -> Option<Spec> {
    match key {
        "MoveVertical" | "ScrollVertical" => spec("browse", "browse", Kind::Ticks, Conv::Id),
        "GoToItem" | "MoveFocusForward" => {
            spec("browse-push", "browse-push", Kind::Button, Conv::Id)
        }
        "MoveFocusBackward" => spec("browse-back", "browse-back", Kind::Button, Conv::Id),
        _ => None,
    }
}

/// The effect units (`[EffectRack1_EffectUnit1]`, `[EffectRack1_EffectUnit1_Effect2]`), the EQ and the quick effect of a channel.
fn effects(group: &str, key: &str) -> Option<Spec> {
    if let Some(rest) = group.strip_prefix("[EffectRack1_EffectUnit") {
        let rest = rest.strip_suffix(']')?;
        let (u, slot) = match rest.split_once("_Effect") {
            Some((u, v)) => (u.parse::<u32>().ok()?, Some(v.parse::<u32>().ok()?)),
            None => (rest.parse().ok()?, None),
        };
        if !(1..=4).contains(&u) {
            return None;
        }
        return match (slot, key) {
            (None, "mix") => {
                let k = format!("fx-level/{u}");
                spec(&k, &k, Kind::Level, Conv::Id)
            }
            (None, _) => {
                // group_[ChannelN]_enable: the unit goes to that channel
                let n = key
                    .strip_prefix("group_[Channel")?
                    .strip_suffix("]_enable")?
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=4).contains(n))?;
                let k = format!("hw-fx-assign/{u}/{}", n - 1);
                spec(&k, &k, Kind::Button, Conv::Id)
            }
            (Some(v @ 1..=3), "enabled") => spec(
                &format!("fx-on/{u}/{}", v - 1),
                &format!("hw-fx-on/{u}/{}", v - 1),
                Kind::Toggle,
                Conv::Id,
            ),
            _ => None,
        };
    }
    if let Some(d) = nested_deck(group, "[EqualizerRack1_", "_Effect1]") {
        let name = match key {
            "parameter1" => "eq-low",
            "parameter2" => "eq-mid",
            "parameter3" => "eq-hi",
            _ => return None,
        };
        let k = format!("{name}/{d}");
        return spec(&k, &k, Kind::Level, Conv::Gain);
    }
    let d = nested_deck(group, "[QuickEffectRack1_", "]")?;
    let k = format!("filter/{d}");
    (key == "super1").then(|| spec(&k, &k, Kind::Level, Conv::Id))?
}

/// The host controls of `[group]` `key`, `None` when it is not translated.
pub fn translate(group: &str, key: &str) -> Option<Spec> {
    match group {
        "[Master]" => master(key),
        "[Library]" => library(key),
        _ => match deck(group) {
            Some(d) => on_deck(d, key),
            None => effects(group, key),
        },
    }
}

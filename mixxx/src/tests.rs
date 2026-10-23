//! The plugin through its C ABI against a recording host: a sample mapping (XML + JS, written for these tests) driven with synthetic MIDI.

use super::*;
use crate::keys::{translate, Conv, Kind};
use openfader_plugin::controller::{api, ControllerApi, Host, Str, CONTROLLER_ABI};
use std::ffi::c_void;

/// What the host saw, behind `Host::ctx`.
#[derive(Default)]
struct Seen {
    set: Vec<(String, f64)>,
    out: Vec<Vec<u8>>,
    log: Vec<String>,
    label: String,
    watched: Vec<String>,
    timers: Vec<(u32, bool)>,
    /// The control values the host answers.
    state: HashMap<String, f64>,
}

fn seen<'a>(ctx: *mut c_void) -> &'a mut Seen {
    unsafe { &mut *ctx.cast::<Seen>() }
}

unsafe extern "C" fn get(ctx: *mut c_void, key: Str) -> f64 {
    let k = unsafe { key.as_str() };
    seen(ctx).state.get(k).copied().unwrap_or(f64::NAN)
}
unsafe extern "C" fn set(ctx: *mut c_void, key: Str, v: f64) -> bool {
    seen(ctx).set.push((unsafe { key.as_str() }.to_owned(), v));
    true
}
unsafe extern "C" fn midi_out(ctx: *mut c_void, m: *const u8, n: usize) {
    seen(ctx)
        .out
        .push(unsafe { std::slice::from_raw_parts(m, n) }.to_vec());
}
unsafe extern "C" fn timer(ctx: *mut c_void, ms: u32, repeat: bool) -> u64 {
    seen(ctx).timers.push((ms, repeat));
    7
}
unsafe extern "C" fn cancel(_: *mut c_void, _: u64) {}
unsafe extern "C" fn watch(ctx: *mut c_void, key: Str, on: bool) {
    if on {
        seen(ctx).watched.push(unsafe { key.as_str() }.to_owned());
    }
}
unsafe extern "C" fn takeover(_: *mut c_void, _: Str, _: bool) {}
unsafe extern "C" fn label(ctx: *mut c_void, t: Str) {
    seen(ctx).label = unsafe { t.as_str() }.to_owned();
}
unsafe extern "C" fn log(ctx: *mut c_void, t: Str) {
    seen(ctx).log.push(unsafe { t.as_str() }.to_owned());
}
unsafe extern "C" fn settings(_: *mut c_void) -> Str {
    static JSON: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        serde_json::json!({"folder": concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data")})
            .to_string()
    });
    Str::new(&JSON)
}

fn host_of(s: &mut Seen) -> Host {
    Host {
        ctx: (s as *mut Seen).cast(),
        get,
        set,
        midi_out,
        timer,
        cancel,
        watch,
        takeover,
        label,
        log,
        settings,
    }
}

static API: ControllerApi = api::<Driver>();

fn send(id: u64, h: &Host, msg: &[u8]) {
    unsafe { (API.midi)(id, msg.as_ptr(), msg.len(), h) }
}

fn changed(id: u64, h: &Host, key: &str, v: f64) {
    unsafe { (API.changed)(id, Str::new(key), v, h) }
}

#[test]
fn a_sample_mapping_drives_the_hosts_controls_and_lights_through_its_scripts() {
    let mut s = Seen::default();
    let h = host_of(&mut s);
    let port = Str::new("Sample Pad MIDI 1 20:0");
    let id = unsafe {
        assert_eq!(API.abi, CONTROLLER_ABI);
        assert!(
            (API.probe)(port, &h) >= 70,
            "the mapping's controller name is in the port's"
        );
        assert_eq!((API.probe)(Str::new("Unrelated Box 21:0"), &h), 0);
        (API.open)(port, false, &h)
    };
    assert_ne!(id, 0, "{:?}", seen(h.ctx).log);

    // the mapping's output and the script's connection watch the lights' controls; init ran
    let s = seen(h.ctx);
    assert_eq!(s.label, "Sample Pad");
    assert!(s.watched.contains(&"at-cue/1".to_owned()));
    assert!(s.watched.contains(&"playing/1".to_owned()));
    assert_eq!(s.timers, [(100, false)]);
    assert!(
        s.out.contains(&vec![0xF0, 0x7E, 0x01, 0xF7]),
        "init sent its sysex"
    );
    assert!(s.log.is_empty(), "{:?}", s.log);
    s.set.clear();

    // play: a press toggles it, the release does nothing
    send(id, &h, &[0x90, 0x01, 127]);
    send(id, &h, &[0x90, 0x01, 0]);
    assert_eq!(
        seen(h.ctx).set,
        [("play/1".to_owned(), 1.0), ("play/1".to_owned(), 0.0)]
    );

    // a hot cue pad follows the button
    seen(h.ctx).set.clear();
    send(id, &h, &[0x90, 0x10, 127]);
    send(id, &h, &[0x80, 0x10, 0]); // its note-off is the note-on with 0
    assert_eq!(
        seen(h.ctx).set,
        [("pad1/1".to_owned(), 1.0), ("pad1/1".to_owned(), 0.0)]
    );

    // the crossfader is absolute, a rate with `invert` is reversed
    seen(h.ctx).set.clear();
    send(id, &h, &[0xB0, 0x07, 127]);
    send(id, &h, &[0xB0, 0x07, 0]);
    send(id, &h, &[0xB0, 0x08, 127]);
    assert_eq!(
        seen(h.ctx).set,
        [
            ("crossfader".to_owned(), 1.0),
            ("crossfader".to_owned(), 0.0),
            ("tempo/1".to_owned(), 0.0)
        ]
    );

    // a JS-scripted jog: touch, turn by 6 and -3 (centred on 64), let go; the `diff` jog reads two's complement
    seen(h.ctx).set.clear();
    send(id, &h, &[0x90, 0x30, 127]);
    send(id, &h, &[0xB0, 0x20, 70]);
    send(id, &h, &[0xB0, 0x20, 61]);
    send(id, &h, &[0x80, 0x30, 0]);
    send(id, &h, &[0xB0, 0x09, 127]);
    assert_eq!(
        seen(h.ctx).set,
        [
            ("jog-touch/1".to_owned(), 1.0),
            ("jog/1".to_owned(), 6.0),
            ("jog/1".to_owned(), -3.0),
            ("jog-touch/1".to_owned(), 0.0),
            ("jog-bend/1".to_owned(), -1.0),
        ]
    );

    // the lights follow: the script's connection and the mapping's output (on from its `minimum`)
    seen(h.ctx).out.clear();
    changed(id, &h, "playing/1", 1.0);
    changed(id, &h, "at-cue/1", 1.0);
    changed(id, &h, "at-cue/1", 0.0);
    changed(id, &h, "playing/1", 0.0);
    assert_eq!(
        seen(h.ctx).out,
        [
            vec![0x90, 0x40, 0x7f],
            vec![0x90, 0x41, 0x7f],
            vec![0x90, 0x41, 0x00],
            vec![0x90, 0x40, 0x00],
        ]
    );

    // the script's timer, then shutdown
    seen(h.ctx).out.clear();
    unsafe { (API.timer)(id, 7, &h) };
    unsafe { (API.close)(id, &h) };
    assert_eq!(seen(h.ctx).out, [vec![0x90, 0x42, 1], vec![0x90, 0x40, 0]]);
    assert!(seen(h.ctx).log.is_empty(), "{:?}", seen(h.ctx).log);
}

#[test]
fn a_script_reads_and_sets_controls_in_mixxx_values() {
    let mut s = Seen::default();
    s.state.insert("fader/1".into(), 0.75);
    s.state.insert("eq-low/1".into(), 0.75);
    s.state.insert("playing/1".into(), 1.0);
    let h = host_of(&mut s);
    let _on = js::enter(99);
    let _ctx = openfader_plugin::controller::host::serving(&h, || {
        let mut js = js::Js::new().unwrap();
        // the volume is a 0 to 1 parameter; an EQ is 0 to 4 with 1 in the middle (0.75 of the knob is 2.5); a set of play only
        // presses when the state differs
        js.eval(
            r#"
            var got = [engine.getValue("[Channel1]", "volume"), engine.getValue("[EqualizerRack1_[Channel1]_Effect1]", "parameter1"),
                       engine.getParameter("[EqualizerRack1_[Channel1]_Effect1]", "parameter1"), engine.getValue("[Channel9]", "play")];
            engine.setValue("[Channel1]", "play", 1);
            engine.setValue("[Channel1]", "pregain", 1);
            engine.setParameter("[Master]", "crossfader", 0.25);
            engine.setValue("[Channel1]", "hotcue_2_activate", 1);
            engine.setValue("[Channel1]", "beatjump_4_backward", 1);
            engine.setValue("[Channel1]", "nonsense", 1);
            midi.sendShortMsg(0xC0, 5, 0);
            print(script.absoluteNonLin(127, 0, 1, 4) + " " + script.deckFromGroup("[Channel3]"));
            "#,
        )
        .unwrap();
        js.eval(r#"print(JSON.stringify(got))"#).unwrap();
    });
    let s = seen(h.ctx);
    assert_eq!(
        s.log[0],
        "control [Channel9] play has no OpenFadeR counterpart"
    );
    assert_eq!(
        s.log[1],
        "control [Channel1] nonsense has no OpenFadeR counterpart"
    );
    assert_eq!(s.log[2], "4 3");
    assert_eq!(s.log[3], "[0.75,2.5,0.75,0]");
    assert_eq!(
        s.set,
        [
            ("trim/1".to_owned(), 0.5),
            ("crossfader".to_owned(), 0.25),
            ("pad2/1".to_owned(), 1.0),
            ("jump-back-4/1".to_owned(), 1.0),
        ]
    );
    assert_eq!(s.out, [vec![0xC0, 5]]);
}

#[test]
fn mixxx_keys_translate_with_their_value_ranges() {
    let play = translate("[Channel2]", "play").unwrap();
    assert_eq!(
        (play.read.as_str(), play.write.as_str(), play.kind),
        ("playing/2", "play/2", Kind::Toggle)
    );
    let cue = translate("[Channel1]", "hotcue_3_clear").unwrap();
    assert_eq!(cue.write, "hw-pad-delete/1/2");
    let fx = translate("[EffectRack1_EffectUnit2_Effect3]", "enabled").unwrap();
    assert_eq!(
        (fx.read.as_str(), fx.write.as_str()),
        ("fx-on/2/2", "hw-fx-on/2/2")
    );
    assert_eq!(
        translate("[QuickEffectRack1_[Channel4]]", "super1")
            .unwrap()
            .write,
        "filter/4"
    );
    assert!(
        translate("[Channel5]", "play").is_none()
            && translate("[Channel1]", "hotcue_9_activate").is_none()
    );
    for c in [Conv::Bipolar, Conv::Gain] {
        for p in [0.0, 0.25, 0.5, 0.9, 1.0] {
            assert!((c.param(c.value(p)) - p).abs() < 1e-9, "{c:?} {p}");
        }
    }
    assert_eq!(Conv::Gain.value(0.5), 1.0);
}

#[test]
fn a_mapping_is_chosen_by_how_much_of_its_name_the_port_has() {
    use mapping::{parse, score};
    assert_eq!(score("Sample Pad", "Sample Pad MIDI 1 20:0"), 100);
    assert_eq!(
        score("Pioneer DDJ-400", "DDJ-400:DDJ-400 MIDI 1 24:0"),
        60,
        "a vendor missing from the port's name is not needed"
    );
    assert_eq!(score("Pioneer DDJ-400", "DDJ-800 MIDI 1"), 0);
    assert_eq!(score("Launchpad Mini", "Launchpad Mini MK2 28:0"), 100);
    assert_eq!(
        score("Launchpad MK2", "Launchpad Mini MK2 28:0"),
        70 + 20,
        "the model words are in it"
    );

    let m = parse(
        std::path::Path::new("x.midi.xml"),
        r#"<MixxxControllerPreset><info><name>X</name></info><controller id="Y"><controls>
           <control><group>[Channel1]</group><key>play</key><status>0x91</status><midino>0x0A</midino>
           <options><fourteen-bit-msb/><soft-takeover/></options></control></controls>
           <outputs><output><group>[Channel1]</group><key>play_indicator</key><status>0x90</status><midino>3</midino></output></outputs>
           </controller></MixxxControllerPreset>"#,
    )
    .unwrap();
    assert_eq!((m.name.as_str(), m.controller.as_str()), ("X", "Y"));
    assert_eq!((m.controls[0].status, m.controls[0].midino), (0x91, 10));
    assert!(m.controls[0].opts.msb && m.controls[0].opts.soft_takeover);
    assert_eq!(
        (
            m.outputs[0].on,
            m.outputs[0].off,
            m.outputs[0].level(0.0),
            m.outputs[0].level(1.0)
        ),
        (0x7f, 0, 0, 0x7f)
    );
    assert!(parse(std::path::Path::new("x"), "<a/>").is_err());
}

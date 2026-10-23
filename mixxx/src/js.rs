//! The embedded JavaScript engine (Boa, pure Rust) with Mixxx's script API: `prelude.js` is the API itself, the natives here reach the
//! host's controls through [`crate::keys`] and `openfader_plugin::controller::host`. The natives take and return numbers and text.

use crate::keys::{translate, Kind, Spec};
use boa_engine::{Context, JsArgs, JsResult, JsString, JsValue, NativeFunction, Source};
use openfader_plugin::controller::host;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// What the natives of one session share: who watches which host control, and the keys already reported as untranslated.
#[derive(Default)]
struct Hub {
    /// Host control → the Mixxx (group, key) pairs that watch it, once per watcher.
    subs: HashMap<String, Vec<(String, String)>>,
    warned: HashSet<String>,
}

thread_local! {
    static HUBS: RefCell<HashMap<u64, Hub>> = RefCell::new(HashMap::new());
    static ACTIVE: Cell<u64> = const { Cell::new(0) };
}

/// The natives serve the session `id` while the guard lives.
pub struct Enter(u64);

pub fn enter(id: u64) -> Enter {
    Enter(ACTIVE.replace(id))
}

impl Drop for Enter {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

fn hub<R>(f: impl FnOnce(&mut Hub) -> R) -> R {
    HUBS.with_borrow_mut(|m| f(m.entry(ACTIVE.get()).or_default()))
}

/// Starts or stops one watcher of Mixxx control (`group`, `key`); the host control is watched while it has any.
pub fn watch(group: &str, key: &str, on: bool) {
    let Some(spec) = translate(group, key) else {
        warn(group, key);
        return;
    };
    hub(|h| {
        let list = h.subs.entry(spec.read.clone()).or_default();
        let pair = (group.to_owned(), key.to_owned());
        if on {
            list.push(pair);
            if list.len() == 1 {
                host::watch(&spec.read, true);
            }
        } else if let Some(at) = list.iter().position(|p| *p == pair) {
            list.remove(at);
            if list.is_empty() {
                h.subs.remove(&spec.read);
                host::watch(&spec.read, false);
            }
        }
    });
}

/// The Mixxx controls watching the host control `key`, each once.
pub fn watchers(key: &str) -> Vec<(String, String)> {
    let mut v = hub(|h| h.subs.get(key).cloned().unwrap_or_default());
    v.sort();
    v.dedup();
    v
}

/// Forgets the session's watchers (it ends).
pub fn clear(id: u64) {
    HUBS.with_borrow_mut(|m| m.remove(&id));
}

/// Says once that a Mixxx control has no counterpart.
pub fn warn(group: &str, key: &str) {
    if hub(|h| h.warned.insert(format!("{group} {key}"))) {
        host::log(&format!("control {group} {key} has no OpenFadeR counterpart"));
    }
}

/// The value of Mixxx control (`group`, `key`), or its 0 to 1 parameter; 0 for one that is not translated.
pub fn read(group: &str, key: &str, param: bool) -> f64 {
    let Some(spec) = translate(group, key) else {
        warn(group, key);
        return 0.0;
    };
    let p = host::get(&spec.read);
    if p.is_nan() {
        0.0
    } else if param {
        p
    } else {
        spec.conv.value(p)
    }
}

/// Sets Mixxx control (`group`, `key`) to a value, or to a 0 to 1 parameter.
pub fn write(group: &str, key: &str, v: f64, param: bool) {
    let Some(spec) = translate(group, key) else {
        warn(group, key);
        return;
    };
    set_spec(&spec, v, param);
}

fn set_spec(spec: &Spec, v: f64, param: bool) {
    match spec.kind {
        Kind::Level => {
            host::set(
                &spec.write,
                if param { v } else { spec.conv.param(v) }.clamp(0.0, 1.0),
            );
        }
        Kind::Button => {
            host::set(&spec.write, f64::from(v > 0.0));
        }
        Kind::Toggle => {
            if (v > 0.0) != (host::get(&spec.read) >= 0.5) {
                host::set(&spec.write, 1.0);
                host::set(&spec.write, 0.0);
            }
        }
        Kind::Ticks => {
            host::set(&spec.write, v.round());
        }
        Kind::Read => {}
    }
}

fn number(a: &[JsValue], i: usize, c: &mut Context) -> JsResult<f64> {
    a.get_or_undefined(i).to_number(c)
}

fn text(a: &[JsValue], i: usize, c: &mut Context) -> JsResult<String> {
    Ok(a.get_or_undefined(i).to_string(c)?.to_std_string_escaped())
}

fn truth(a: &[JsValue], i: usize) -> bool {
    a.get_or_undefined(i).to_boolean()
}

fn undefined() -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}

fn n_get(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (g, k) = (text(a, 0, c)?, text(a, 1, c)?);
    Ok(read(&g, &k, truth(a, 2)).into())
}

fn n_set(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (g, k, v) = (text(a, 0, c)?, text(a, 1, c)?, number(a, 2, c)?);
    write(&g, &k, v, truth(a, 3));
    undefined()
}

fn n_param(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (g, k, v) = (text(a, 0, c)?, text(a, 1, c)?, number(a, 2, c)?);
    Ok(translate(&g, &k).map_or(0.0, |s| s.conv.param(v)).into())
}

fn n_watch(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (g, k) = (text(a, 0, c)?, text(a, 1, c)?);
    watch(&g, &k, truth(a, 2));
    undefined()
}

fn n_soft(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (g, k) = (text(a, 0, c)?, text(a, 1, c)?);
    if let Some(s) = translate(&g, &k).filter(|s| s.kind == Kind::Level) {
        host::takeover(&s.write, truth(a, 2));
    }
    undefined()
}

fn n_timer(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    // Mixxx's shortest timer is 20 ms
    let ms = number(a, 0, c)?.max(20.0) as u32;
    Ok((host::timer(ms, !truth(a, 1)).unwrap_or(0) as f64).into())
}

fn n_cancel(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    host::cancel(number(a, 0, c)? as u64);
    undefined()
}

fn n_touch(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    host::set(
        &format!("jog-touch/{}", number(a, 0, c)?),
        f64::from(truth(a, 1)),
    );
    undefined()
}

fn n_jog(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    host::set(
        &format!("jog/{}", number(a, 0, c)?),
        number(a, 1, c)?.round(),
    );
    undefined()
}

fn n_brake(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let key = format!("brake/{}", number(a, 0, c)?);
    host::set(&key, 1.0);
    host::set(&key, 0.0);
    undefined()
}

fn n_midi(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let (s, d1, d2) = (number(a, 0, c)?, number(a, 1, c)?, number(a, 2, c)?);
    let m = [s as u8, d1 as u8, d2 as u8];
    // program change and channel pressure have one data byte
    host::midi_out(if matches!(m[0] & 0xF0, 0xC0 | 0xD0) {
        &m[..2]
    } else {
        &m
    });
    undefined()
}

fn n_sysex(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    let bytes: Vec<u8> = text(a, 0, c)?
        .split(',')
        .filter_map(|b| b.trim().parse::<f64>().ok())
        .map(|b| b as u8)
        .collect();
    host::midi_out(&bytes);
    undefined()
}

fn n_print(_: &JsValue, a: &[JsValue], c: &mut Context) -> JsResult<JsValue> {
    host::log(&text(a, 0, c)?);
    undefined()
}

/// A script engine with the Mixxx API loaded.
pub struct Js {
    ctx: Context,
}

impl Js {
    pub fn new() -> Result<Js, String> {
        let mut ctx = Context::default();
        let natives: [(
            &str,
            usize,
            fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ); 13] = [
            ("__get", 3, n_get),
            ("__set", 4, n_set),
            ("__param", 3, n_param),
            ("__watch", 3, n_watch),
            ("__soft", 3, n_soft),
            ("__timer", 2, n_timer),
            ("__cancel", 1, n_cancel),
            ("__touch", 2, n_touch),
            ("__jog", 2, n_jog),
            ("__brake", 1, n_brake),
            ("__midi", 3, n_midi),
            ("__sysexOut", 1, n_sysex),
            ("__print", 1, n_print),
        ];
        for (name, len, f) in natives {
            ctx.register_global_callable(JsString::from(name), len, NativeFunction::from_fn_ptr(f))
                .map_err(|e| e.to_string())?;
        }
        let mut js = Js { ctx };
        js.eval(include_str!("prelude.js"))?;
        Ok(js)
    }

    /// Runs script text in the global scope.
    pub fn eval(&mut self, src: &str) -> Result<(), String> {
        self.ctx
            .eval(Source::from_bytes(src))
            .map(drop)
            .map_err(|e| e.to_string())
    }

    /// Calls the global function `name` with `args`.
    pub fn call(&mut self, name: &str, args: &[JsValue]) -> Result<JsValue, String> {
        let f = self
            .ctx
            .global_object()
            .get(JsString::from(name), &mut self.ctx)
            .map_err(|e| e.to_string())?;
        let f = f
            .as_callable()
            .ok_or_else(|| format!("{name} is not a function"))?;
        f.call(&JsValue::undefined(), args, &mut self.ctx)
            .map_err(|e| format!("{name}: {e}"))
    }
}

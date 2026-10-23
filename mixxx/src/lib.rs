//! Runs Mixxx controller mappings (`*.midi.xml` and their JavaScript files, read from the user's own Mixxx folders) on a MIDI port as an
//! OpenFadeR controller plugin. The port's messages go to the mapping's controls and script functions; the script API (`engine`,
//! `midi`, `script`) is on top of the host's controls ([`keys`], [`js`]); the mapping's outputs and `makeConnection` callbacks follow
//! the controls the host reports as changed.

mod js;
mod keys;
mod mapping;

#[cfg(test)]
mod tests;

use boa_engine::JsValue;
use mapping::{Mapping, Relative};
use openfader_plugin::controller::{host, Controller, ControllerPlugin};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

openfader_plugin::export_controller!(Driver);

struct Driver;

thread_local! {
    /// The mappings found for the folder setting they were found with: `probe` runs for every port at every connect.
    static LIBRARY: RefCell<Option<(String, Rc<Vec<Mapping>>)>> = const { RefCell::new(None) };
}

fn setting(name: &str) -> String {
    serde_json::from_str::<serde_json::Value>(&host::settings())
        .ok()
        .and_then(|v| v.get(name).and_then(|s| s.as_str().map(str::to_owned)))
        .unwrap_or_default()
}

/// The mappings of the user's folders; scanned again (`fresh`) when a port is opened.
fn library(fresh: bool) -> (Rc<Vec<Mapping>>, Vec<PathBuf>) {
    let custom = setting("folder");
    let folders = mapping::folders(&custom);
    let cached = LIBRARY.with_borrow(|l| {
        l.as_ref()
            .filter(|(c, _)| !fresh && *c == custom)
            .map(|(_, m)| m.clone())
    });
    if let Some(m) = cached {
        return (m, folders);
    }
    let mut skipped = Vec::new();
    let maps = Rc::new(mapping::scan(&folders, &mut skipped));
    for s in skipped {
        host::log(&format!("skipped {s}"));
    }
    LIBRARY.with_borrow_mut(|l| *l = Some((custom, maps.clone())));
    (maps, folders)
}

impl ControllerPlugin for Driver {
    fn probe(port: &str) -> u32 {
        mapping::best(&library(false).0, port).map_or(0, |(_, s)| s)
    }

    fn open(port: &str, forced: bool) -> Result<Box<dyn Controller>, String> {
        let (maps, folders) = library(true);
        let wanted = if forced {
            mapping::named(&maps, &setting("mapping"))
        } else {
            None
        };
        let m = wanted
            .or_else(|| mapping::best(&maps, port).map(|(m, _)| m))
            .ok_or_else(|| {
                format!(
                    "no Mixxx mapping matches {port} ({} mappings in {} folders; name one in the plugin's Mapping setting)",
                    maps.len(),
                    folders.iter().filter(|f| f.is_dir()).count()
                )
            })?;
        Ok(Box::new(Session::new(m, &folders)?))
    }
}

/// One claimed port: the mapping, its script engine and what the outputs last showed.
struct Session {
    id: u64,
    js: js::Js,
    map: Mapping,
    /// (status, midino) → the controls that listen to it.
    table: HashMap<(u8, u8), Vec<usize>>,
    /// The two halves of 14-bit controls, by (group, key): (msb, lsb).
    wide: HashMap<(String, String), (u8, u8)>,
    /// The data byte each output last sent.
    sent: Vec<Option<u8>>,
}

fn script_error(what: &str, e: &str) {
    host::log(&format!("{what}: {e}"));
}

impl Session {
    fn new(map: &Mapping, folders: &[PathBuf]) -> Result<Session, String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let _on = js::enter(id);
        let mut js = js::Js::new()?;
        let dir = map.file.parent().map(PathBuf::from).unwrap_or_default();
        for (file, _) in &map.scripts {
            let found = std::iter::once(&dir)
                .chain(folders)
                .map(|d| d.join(file))
                .find(|p| p.is_file());
            match found
                .map(|p| std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display())))
            {
                Some(Ok(src)) => {
                    if let Err(e) = js.eval(&src) {
                        script_error(file, &e);
                    }
                }
                Some(Err(e)) => script_error(file, &e),
                None => script_error(file, "script file not found"),
            }
        }
        let mut table: HashMap<(u8, u8), Vec<usize>> = HashMap::new();
        for (i, c) in map.controls.iter().enumerate() {
            table.entry((c.status, c.midino)).or_default().push(i);
            if c.opts.script {
                if let Err(e) = js.call(
                    "__bind",
                    &[JsValue::from(i as f64), JsValue::from(js_str(&c.key))],
                ) {
                    script_error(&c.key, &e);
                }
            } else if c.opts.soft_takeover {
                if let Some(s) = keys::translate(&c.group, &c.key) {
                    host::takeover(&s.write, true);
                }
            }
        }
        for o in &map.outputs {
            js::watch(&o.group, &o.key, true);
        }
        host::label(&map.name);
        for (_, prefix) in &map.scripts {
            let args = [
                JsValue::from(js_str(prefix)),
                JsValue::from(js_str("init")),
                JsValue::from(js_str(&map.controller)),
                JsValue::from(false),
            ];
            if let Err(e) = js.call("__call", &args) {
                script_error(prefix, &e);
            }
        }
        Ok(Session {
            id,
            js,
            map: map.clone(),
            table,
            wide: HashMap::new(),
            sent: vec![None; map.outputs.len()],
        })
    }

    /// A control with no script: the message's value goes to the OpenFadeR control by its kind and the control's options.
    fn direct(&mut self, i: usize, value: u16, max: f64) {
        let c = &self.map.controls[i];
        let Some(spec) = keys::translate(&c.group, &c.key) else {
            js::warn(&c.group, &c.key);
            return;
        };
        let mut v = f64::from(value);
        if c.opts.msb || c.opts.lsb {
            let e = self
                .wide
                .entry((c.group.clone(), c.key.clone()))
                .or_default();
            if c.opts.msb {
                e.0 = value as u8;
            } else {
                e.1 = value as u8;
            }
            v = f64::from(u16::from(e.0) << 7 | u16::from(e.1));
        }
        let max = if c.opts.msb || c.opts.lsb {
            16383.0
        } else {
            max
        };
        let ticks = |default| c.opts.relative.unwrap_or(default).ticks(value as u8);
        match spec.kind {
            keys::Kind::Read => {}
            keys::Kind::Ticks => {
                host::set(&spec.write, f64::from(ticks(Relative::TwosComplement)));
            }
            keys::Kind::Toggle => {
                if v > 0.0 {
                    host::set(&spec.write, 1.0);
                    host::set(&spec.write, 0.0);
                }
            }
            keys::Kind::Button => {
                host::set(&spec.write, f64::from(v > 0.0));
            }
            keys::Kind::Level => {
                let p = match c.opts.relative {
                    Some(r) => {
                        let now = host::get(&spec.read);
                        let now = if now.is_nan() { 0.0 } else { now };
                        now + f64::from(r.ticks(value as u8)) / 127.0
                    }
                    None if c.opts.invert => 1.0 - v / max,
                    None => v / max,
                };
                host::set(&spec.write, p.clamp(0.0, 1.0));
            }
        }
    }

    fn run(&mut self, i: usize, status: u8, midino: u8, value: u16, max: f64) {
        let c = &self.map.controls[i];
        if !c.opts.script {
            return self.direct(i, value, max);
        }
        let args = [
            JsValue::from(i as f64),
            JsValue::from(f64::from(status & 0x0F)),
            JsValue::from(f64::from(midino)),
            JsValue::from(f64::from(value)),
            JsValue::from(f64::from(status)),
            JsValue::from(js_str(&c.group)),
        ];
        if let Err(e) = self.js.call("__in", &args) {
            script_error("script", &e);
        }
    }

    fn sysex(&mut self, msg: &[u8]) {
        let bytes = msg.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
        let bytes = JsValue::from(js_str(&bytes));
        for i in 0..self.map.controls.len() {
            let c = &self.map.controls[i];
            if c.status == 0xF0 && c.opts.script {
                if let Err(e) = self
                    .js
                    .call("__sysex", &[JsValue::from(i as f64), bytes.clone()])
                {
                    script_error("sysex", &e);
                }
            }
        }
        for (_, prefix) in &self.map.scripts {
            let args = [JsValue::from(js_str(prefix)), bytes.clone()];
            if let Err(e) = self.js.call("__incoming", &args) {
                script_error("incomingData", &e);
            }
        }
    }
}

fn js_str(s: &str) -> boa_engine::JsString {
    boa_engine::JsString::from(s)
}

impl Controller for Session {
    fn midi(&mut self, msg: &[u8]) {
        let _on = js::enter(self.id);
        let Some(&status) = msg.first() else { return };
        if status == 0xF0 {
            return self.sysex(msg);
        }
        let (d1, d2) = (
            msg.get(1).copied().unwrap_or(0),
            msg.get(2).copied().unwrap_or(0),
        );
        // inferred: a pitch bend is control 0 with its 14-bit value, a program change its number with 127, channel pressure control 0
        let (midino, value, max) = match status & 0xF0 {
            0xE0 => (0, u16::from(d2) << 7 | u16::from(d1), 16383.0),
            0xC0 => (d1, 127, 127.0),
            0xD0 => (0, u16::from(d1), 127.0),
            _ => (d1, u16::from(d2), 127.0),
        };
        let mut hit = self.table.get(&(status, midino)).cloned();
        let mut value = value;
        if hit.is_none() && status & 0xF0 == 0x80 {
            // a mapping written for note-on listens to its note-off too, as a value of 0
            hit = self.table.get(&(status + 0x10, midino)).cloned();
            value = 0;
        }
        for i in hit.unwrap_or_default() {
            self.run(i, status, midino, value, max);
        }
    }

    fn changed(&mut self, key: &str, value: f64) {
        let _on = js::enter(self.id);
        for (g, k) in js::watchers(key) {
            let Some(spec) = keys::translate(&g, &k) else {
                continue;
            };
            let v = spec.conv.value(value);
            for (i, o) in self.map.outputs.iter().enumerate() {
                if o.group == g && o.key == k {
                    let b = o.level(v);
                    if self.sent[i] != Some(b) {
                        self.sent[i] = Some(b);
                        host::midi_out(&[o.status, o.midino, b]);
                    }
                }
            }
            let args = [
                JsValue::from(js_str(&g)),
                JsValue::from(js_str(&k)),
                JsValue::from(v),
            ];
            if let Err(e) = self.js.call("__changed", &args) {
                script_error("connection", &e);
            }
        }
    }

    fn timer(&mut self, id: u64) {
        let _on = js::enter(self.id);
        if let Err(e) = self.js.call("__tick", &[JsValue::from(id as f64)]) {
            script_error("timer", &e);
        }
    }

    fn close(&mut self) {
        let _on = js::enter(self.id);
        for (_, prefix) in &self.map.scripts {
            let args = [
                JsValue::from(js_str(prefix)),
                JsValue::from(js_str("shutdown")),
            ];
            if let Err(e) = self.js.call("__call", &args) {
                script_error(prefix, &e);
            }
        }
        js::clear(self.id);
    }
}

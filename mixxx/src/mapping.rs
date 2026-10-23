//! Mixxx's controller mapping files (`*.midi.xml`, as the Mixxx manual documents them): the folders to look in, the file parser and
//! the choice of the mapping for a port. Nothing of Mixxx is bundled: the files are read from the user's own folders.

use std::path::{Path, PathBuf};

/// What `<options>` of a `<control>` says; the value of a message is read the way the first flag that applies tells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    pub script: bool,
    pub invert: bool,
    /// Two's complement steps (`diff`, `selectknob`, `hercjog`) or centred on 64 (`spread64`, `rot64`): signed ticks.
    pub relative: Option<Relative>,
    pub button: bool,
    pub soft_takeover: bool,
    pub msb: bool,
    pub lsb: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Relative {
    TwosComplement,
    Centred,
    CentredInverted,
}

impl Relative {
    pub fn ticks(self, v: u8) -> i32 {
        let v = i32::from(v);
        match self {
            Relative::TwosComplement if v >= 64 => v - 128,
            Relative::TwosComplement => v,
            Relative::Centred => v - 64,
            Relative::CentredInverted => 64 - v,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub group: String,
    pub key: String,
    pub status: u8,
    pub midino: u8,
    pub opts: Options,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub group: String,
    pub key: String,
    pub status: u8,
    pub midino: u8,
    pub on: u8,
    pub off: u8,
    pub min: Option<f64>,
    pub max: f64,
}

impl Output {
    /// The data byte that shows control value `v`. Inferred from the manual: on within `minimum` .. `maximum` (without a
    /// `minimum`, above 0), off outside.
    pub fn level(&self, v: f64) -> u8 {
        let on = v <= self.max && self.min.map_or(v > 0.0, |m| v >= m);
        if on {
            self.on
        } else {
            self.off
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mapping {
    pub file: PathBuf,
    pub name: String,
    /// `<controller id>`: the controller's name.
    pub controller: String,
    /// Script files with their function prefix.
    pub scripts: Vec<(String, String)>,
    pub controls: Vec<Control>,
    pub outputs: Vec<Output>,
}

fn number(s: &str) -> Option<u8> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u8::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn text<'a>(n: roxmltree::Node<'a, '_>, tag: &str) -> Option<&'a str> {
    n.children()
        .find(|c| c.has_tag_name(tag))
        .and_then(|c| c.text())
        .map(str::trim)
}

fn options(n: roxmltree::Node) -> Options {
    let mut o = Options::default();
    let Some(list) = n.children().find(|c| c.has_tag_name("options")) else {
        return o;
    };
    for c in list.children().filter(roxmltree::Node::is_element) {
        match c.tag_name().name().to_ascii_lowercase().as_str() {
            "script-binding" => o.script = true,
            "invert" => o.invert = true,
            "diff" | "selectknob" | "hercjog" => o.relative = Some(Relative::TwosComplement),
            "spread64" | "rot64" | "rot64fast" => o.relative = Some(Relative::Centred),
            "rot64inv" => o.relative = Some(Relative::CentredInverted),
            "button" | "switch" => o.button = true,
            "soft-takeover" => o.soft_takeover = true,
            "fourteen-bit-msb" => o.msb = true,
            "fourteen-bit-lsb" => o.lsb = true,
            _ => {}
        }
    }
    o
}

/// Parses a mapping file's text; `file` is where it came from.
pub fn parse(file: &Path, xml: &str) -> Result<Mapping, String> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let mut m = Mapping {
        file: file.to_owned(),
        ..Mapping::default()
    };
    for n in doc.descendants().filter(roxmltree::Node::is_element) {
        match n.tag_name().name() {
            "info" => m.name = text(n, "name").unwrap_or_default().to_owned(),
            "controller" => {
                if m.controller.is_empty() {
                    m.controller = n.attribute("id").unwrap_or_default().to_owned();
                }
            }
            "file" if n.parent().is_some_and(|p| p.has_tag_name("scriptfiles")) => {
                if let Some(f) = n.attribute("filename") {
                    let prefix = n.attribute("functionprefix").unwrap_or_default();
                    m.scripts.push((f.to_owned(), prefix.to_owned()));
                }
            }
            "control" => {
                let (Some(group), Some(key)) = (text(n, "group"), text(n, "key")) else {
                    continue;
                };
                let (Some(status), Some(midino)) = (
                    text(n, "status").and_then(number),
                    text(n, "midino").and_then(number),
                ) else {
                    continue;
                };
                m.controls.push(Control {
                    group: group.to_owned(),
                    key: key.to_owned(),
                    status,
                    midino,
                    opts: options(n),
                });
            }
            "output" => {
                let (Some(group), Some(key)) = (text(n, "group"), text(n, "key")) else {
                    continue;
                };
                let (Some(status), Some(midino)) = (
                    text(n, "status").and_then(number),
                    text(n, "midino").and_then(number),
                ) else {
                    continue;
                };
                let float = |tag| text(n, tag).and_then(|t| t.parse::<f64>().ok());
                m.outputs.push(Output {
                    group: group.to_owned(),
                    key: key.to_owned(),
                    status,
                    midino,
                    on: text(n, "on").and_then(number).unwrap_or(0x7f),
                    off: text(n, "off").and_then(number).unwrap_or(0),
                    min: float("minimum"),
                    max: float("maximum").unwrap_or(1.0),
                });
            }
            _ => {}
        }
    }
    if m.name.is_empty() {
        m.name.clone_from(&m.controller);
    }
    if m.controls.is_empty() && m.outputs.is_empty() && m.scripts.is_empty() {
        return Err("no controls, outputs or scripts".to_owned());
    }
    Ok(m)
}

/// The folders a Mixxx mapping lives in: the user's own, then the usual per-OS install and user paths.
pub fn folders(custom: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if !custom.trim().is_empty() {
        v.push(PathBuf::from(custom.trim()));
    }
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(home) = env("HOME").or_else(|| env("USERPROFILE")) {
        v.push(home.join(".mixxx/controllers"));
        v.push(home.join(".var/app/org.mixxx.Mixxx/.mixxx/controllers"));
        v.push(home.join("Library/Application Support/Mixxx/controllers"));
    }
    if let Some(d) = env("LOCALAPPDATA") {
        v.push(d.join("Mixxx/controllers"));
    }
    for d in [
        "/usr/share/mixxx/controllers",
        "/usr/local/share/mixxx/controllers",
        "/opt/mixxx/share/mixxx/controllers",
        "/var/lib/flatpak/app/org.mixxx.Mixxx/current/active/files/share/mixxx/controllers",
        "/Applications/Mixxx.app/Contents/Resources/controllers",
        "C:/Program Files/Mixxx/controllers",
    ] {
        v.push(PathBuf::from(d));
    }
    v
}

/// Every `*.midi.xml` of `folders` that parses, in folder order (files by name); a file that does not is listed in `skipped`.
pub fn scan(folders: &[PathBuf], skipped: &mut Vec<String>) -> Vec<Mapping> {
    let mut all = Vec::new();
    for dir in folders {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().ends_with(".midi.xml"))
            })
            .collect();
        files.sort();
        for f in files {
            match std::fs::read_to_string(&f)
                .map_err(|e| e.to_string())
                .and_then(|x| parse(&f, &x))
            {
                Ok(m) => all.push(m),
                Err(e) => skipped.push(format!("{}: {e}", f.display())),
            }
        }
    }
    all
}

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// How sure a mapping named `name` drives the port `port`, 0 to 100 (inferred, a port's name is rarely a mapping's name): the whole
/// name inside the port's is 100; every word of the name inside it is 70 (90 with a model number among them); at least half of the
/// words with every model number is 60.
pub fn score(name: &str, port: &str) -> u32 {
    let port = squash(port);
    let w = words(name);
    if w.is_empty() {
        return 0;
    }
    if port.contains(&squash(name)) {
        return 100;
    }
    let hit = |x: &&String| port.contains(x.as_str());
    let numbers: Vec<&String> = w
        .iter()
        .filter(|x| x.chars().any(|c| c.is_ascii_digit()) && x.len() >= 2)
        .collect();
    let matched = w.iter().filter(hit).count();
    let all_numbers = !numbers.is_empty() && numbers.iter().all(hit);
    if matched == w.len() {
        70 + 20 * u32::from(all_numbers)
    } else if all_numbers && matched * 2 >= w.len() {
        60
    } else {
        0
    }
}

/// The best mapping for `port` and its score; the first of equals.
pub fn best<'a>(maps: &'a [Mapping], port: &str) -> Option<(&'a Mapping, u32)> {
    let stem = |m: &Mapping| {
        m.file
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| {
                n.trim_end_matches(".xml")
                    .trim_end_matches(".midi")
                    .to_owned()
            })
            .unwrap_or_default()
    };
    let mut top: Option<(&Mapping, u32)> = None;
    for m in maps {
        let s = [m.controller.clone(), m.name.clone(), stem(m)]
            .iter()
            .map(|n| score(n, port))
            .max()
            .unwrap_or(0);
        if s > 0 && top.is_none_or(|(_, t)| s > t) {
            top = Some((m, s));
        }
    }
    top
}

/// A mapping whose name or file name contains `part` (case blind).
pub fn named<'a>(maps: &'a [Mapping], part: &str) -> Option<&'a Mapping> {
    let part = part.trim().to_lowercase();
    (!part.is_empty())
        .then(|| {
            maps.iter().find(|m| {
                [
                    m.name.as_str(),
                    m.controller.as_str(),
                    &m.file.to_string_lossy(),
                ]
                .iter()
                .any(|n| n.to_lowercase().contains(&part))
            })
        })
        .flatten()
}

//! `sampler_pack.xml` (observed in the factory pack): `<TRACKS>` of `<TRACK id location mode bpm/>`, then `<LISTS>` as nested
//! `<LIST id name attr seq>` holding `<TRACK key=id/>`. A list with tracks is a playlist, one without a folder (inferred from
//! the one pack seen; `attr` is ignored).

use rekordbox_common::{Edge, tags};

/// One `<TRACK>` of the pack.
#[derive(Debug, PartialEq)]
pub struct PackTrack {
    pub id: String,
    pub location: String,
    pub bpm_x100: i64,
}

/// A `<LIST>`: `tracks` are `PackTrack::id`s (empty for a folder); `depth` 0 is the pack's top list.
#[derive(Debug, PartialEq)]
pub struct List {
    pub name: String,
    pub depth: usize,
    pub tracks: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct Pack {
    pub tracks: Vec<PackTrack>,
    pub lists: Vec<List>,
}

/// Parses `sampler_pack.xml`.
pub fn parse(xml: &str) -> Result<Pack, String> {
    let mut pack = Pack {
        tracks: Vec::new(),
        lists: Vec::new(),
    };
    let (mut in_lists, mut depth, mut open) = (false, 0usize, Vec::<usize>::new());
    for t in tags(xml)? {
        let a = |k: &str| t.attrs.get(k).cloned().unwrap_or_default();
        match (t.name.as_str(), t.edge) {
            ("LISTS", Edge::Open) => in_lists = true,
            ("LISTS", Edge::Close) => in_lists = false,
            ("TRACK", e) if !in_lists && e != Edge::Close => {
                let location = a("location");
                if location.is_empty() {
                    return Err("a TRACK of sampler_pack.xml has no location".into());
                }
                pack.tracks.push(PackTrack {
                    id: a("id"),
                    location,
                    bpm_x100: (a("bpm").trim().parse::<f64>().unwrap_or(0.0) * 100.0).round()
                        as i64,
                });
            }
            ("TRACK", Edge::Empty | Edge::Open) => {
                if let Some(&l) = open.last() {
                    pack.lists[l].tracks.push(a("key"));
                }
            }
            ("LIST", Edge::Close) => {
                open.pop();
                depth = depth.saturating_sub(1);
            }
            ("LIST", e) => {
                pack.lists.push(List {
                    name: a("name"),
                    depth,
                    tracks: Vec::new(),
                });
                if e == Edge::Open {
                    open.push(pack.lists.len() - 1);
                    depth += 1;
                }
            }
            _ => {}
        }
    }
    if pack.tracks.is_empty() {
        return Err("the pack lists no tracks".into());
    }
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const XML_TEXT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PACK Version="1.0.0"><TRACKS num="2">
<TRACK id="1" location="Kit/a.wav" mode="1" bpm="123.50" />
<TRACK id="2" location="Kit/b.wav" mode="1" bpm="90.00" />
</TRACKS><LISTS><LIST id="1" name="My Pack" attr="3" seq="0">
<LIST id="2" name="Kit &amp; Co" attr="2" seq="0"><TRACK key="2" /><TRACK key="1" /></LIST>
</LIST></LISTS></PACK>"#;

    #[test]
    fn the_xml_gives_tracks_with_bpm_and_the_list_tree() {
        let p = parse(XML_TEXT).unwrap();
        assert_eq!(
            (p.tracks[0].bpm_x100, p.tracks[1].location.as_str()),
            (12350, "Kit/b.wav")
        );
        assert_eq!(p.lists.len(), 2);
        assert_eq!((p.lists[0].depth, p.lists[0].tracks.len()), (0, 0));
        assert_eq!(
            (
                p.lists[1].name.as_str(),
                p.lists[1].depth,
                p.lists[1].tracks.clone()
            ),
            ("Kit & Co", 1, vec!["2".into(), "1".into()])
        );
        assert!(parse("<PACK><TRACKS/></PACK>").is_err());
    }
}

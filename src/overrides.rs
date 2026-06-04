/// Persistent manual overrides — loaded from and saved to data/overrides.json.
///
/// Overrides win over every other data source (live HLTV or synthetic).
/// The JSON file is also read by the Python pipeline on every fetch+train run.
///
/// Serde notes:
///  • The Python template puts `_doc` string keys inside the `teams` and `players`
///    objects; custom deserializers skip those.
///  • Player entries from Python are arrays `[name, r, kd, adr, kast, od]`;
///    the GUI writes objects `{name, rating, …}`.  Both are handled.

use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

// ── Custom map deserializers ──────────────────────────────────────────────────

fn deser_team_map<'de, D>(d: D) -> Result<HashMap<String, TeamOverride>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(d)?;
    let mut out = HashMap::new();
    for (k, v) in raw {
        if k.starts_with('_') || !v.is_object() {
            continue;
        }
        if let Ok(entry) = serde_json::from_value::<TeamOverride>(v) {
            out.insert(k, entry);
        }
    }
    Ok(out)
}

fn deser_player_map<'de, D>(d: D) -> Result<HashMap<String, Vec<PlayerEntry>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(d)?;
    let mut out = HashMap::new();
    for (k, v) in raw {
        if k.starts_with('_') {
            continue;
        }
        if let serde_json::Value::Array(arr) = v {
            let players: Vec<PlayerEntry> = arr
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect();
            if !players.is_empty() {
                out.insert(k, players);
            }
        }
    }
    Ok(out)
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Overrides {
    #[serde(default, deserialize_with = "deser_team_map")]
    pub teams: HashMap<String, TeamOverride>,
    #[serde(default, deserialize_with = "deser_player_map")]
    pub players: HashMap<String, Vec<PlayerEntry>>,
    #[serde(default)]
    pub results: ResultsOverride,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct TeamOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_rating: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_wr_1m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_wr_3m: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_wr_6m: Option<f32>,
}

impl TeamOverride {
    pub fn is_empty(&self) -> bool {
        self.avg_rating.is_none()
            && self.recent_wr_1m.is_none()
            && self.recent_wr_3m.is_none()
            && self.recent_wr_6m.is_none()
    }
}

/// A single player entry.  Custom Serialize always writes object form so the
/// Python pipeline can read it back.  Custom Deserialize accepts both the
/// legacy array form `[name, r, kd, adr, kast, od]` and the new object form.
#[derive(Clone, Debug, Default)]
pub struct PlayerEntry {
    pub name: String,
    pub rating: f32,
    pub kd: f32,
    pub adr: f32,
    pub kast: f32,
    pub od_success: f32,
}

impl Serialize for PlayerEntry {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("PlayerEntry", 6)?;
        st.serialize_field("name",       &self.name)?;
        st.serialize_field("rating",     &self.rating)?;
        st.serialize_field("kd",         &self.kd)?;
        st.serialize_field("adr",        &self.adr)?;
        st.serialize_field("kast",       &self.kast)?;
        st.serialize_field("od_success", &self.od_success)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for PlayerEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, SeqAccess, Visitor};

        struct PV;
        impl<'de> Visitor<'de> for PV {
            type Value = PlayerEntry;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "player as [name,r,kd,adr,kast,od] or {{name,rating,...}}")
            }

            // ── Array form: [name, rating, kd, adr, kast, od_success] ─────────
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<PlayerEntry, A::Error> {
                let name:  String = s.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let rating: f32   = s.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let kd:     f32   = s.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let adr:    f32   = s.next_element()?.ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let kast:   f32   = s.next_element()?.ok_or_else(|| de::Error::invalid_length(4, &self))?;
                let od:     f32   = s.next_element()?.ok_or_else(|| de::Error::invalid_length(5, &self))?;
                Ok(PlayerEntry { name, rating, kd, adr, kast, od_success: od })
            }

            // ── Object form: {name, rating, kd, adr, kast, od_success} ────────
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<PlayerEntry, A::Error> {
                let (mut name, mut rating, mut kd, mut adr, mut kast, mut od) =
                    (None::<String>, None::<f32>, None::<f32>, None::<f32>, None::<f32>, None::<f32>);
                while let Some(key) = m.next_key::<String>()? {
                    match key.as_str() {
                        "name"       => name   = Some(m.next_value()?),
                        "rating"     => rating = Some(m.next_value()?),
                        "kd"         => kd     = Some(m.next_value()?),
                        "adr"        => adr    = Some(m.next_value()?),
                        "kast"       => kast   = Some(m.next_value()?),
                        "od_success" => od     = Some(m.next_value()?),
                        _            => { m.next_value::<serde_json::Value>()?; }
                    }
                }
                Ok(PlayerEntry {
                    name:       name.ok_or_else(|| de::Error::missing_field("name"))?,
                    rating:     rating.unwrap_or(1.0),
                    kd:         kd.unwrap_or(1.0),
                    adr:        adr.unwrap_or(65.0),
                    kast:       kast.unwrap_or(72.0),
                    od_success: od.unwrap_or(0.49),
                })
            }
        }

        d.deserialize_any(PV)
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ResultsOverride {
    #[serde(default)]
    pub matches: Vec<MatchEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatchEntry {
    pub team_a: String,
    pub team_b: String,
    pub winner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
}

// ── Load / save ───────────────────────────────────────────────────────────────

impl Overrides {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("overrides.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| match serde_json::from_str(&s) {
                Ok(v)  => Some(v),
                Err(e) => { eprintln!("overrides.json parse warning: {e}"); None }
            })
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let path = data_dir.join("overrides.json");
        serde_json::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()))
    }
}

//! Session history: every run of `r3dsims live` appends its results (rounds won and lost, draws, matches) to a
//! JSON-lines file, so the record survives restarts and crashes. One line per event:
//!
//! ```text
//! {"t":1700000000,"session":"1700000000-4242","ev":"start","title":"..","sides":{"A":"RAKYAT","B":"PEMERINTAH"}}
//! {"t":..,"session":"..","ev":"round","round":1,"winner":"A","hp":{"A":300,"B":0},"units":{..},"biome":"siang","round_secs":61.2}
//! {"t":..,"session":"..","ev":"match","winner":"B","rounds":3}
//! {"t":..,"session":"..","ev":"hb"}                     (every 30 s, so a killed session still has a duration)
//! ```
//!
//! `summary` folds the file into per-session tallies and totals per side name.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};

pub struct History {
    path: PathBuf,
    id: String,
    warned: Mutex<bool>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl History {
    /// Starts a session: appends its `start` line. `sides` = (key, display name) of both camps.
    pub fn open(path: PathBuf, title: &str, sides: [(String, String); 2]) -> History {
        let h = History { path, id: format!("{}-{}", now(), std::process::id()), warned: Mutex::new(false) };
        h.append(json!({"ev": "start", "title": title, "sides": { sides[0].0.clone(): sides[0].1.clone(), sides[1].0.clone(): sides[1].1.clone() }}));
        h
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Appends one event of this session. A file that cannot be written is reported once and never stops the stream.
    pub fn append(&self, mut ev: Value) {
        ev["t"] = json!(now());
        ev["session"] = json!(self.id);
        let res = (|| -> std::io::Result<()> {
            if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
            writeln!(f, "{ev}")
        })();
        if let Err(e) = res {
            let mut w = self.warned.lock().unwrap_or_else(|e| e.into_inner());
            if !*w {
                eprintln!("[warn] cannot write the history file {}: {e}", self.path.display());
                *w = true;
            }
        }
    }

    /// The whole file folded into sessions (newest first, at most 50) and totals.
    pub fn summary(&self) -> Value {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut v = aggregate(&text, &self.id);
        v["enabled"] = json!(true);
        v["file"] = json!(self.path.display().to_string());
        v
    }
}

#[derive(Default)]
struct Sess {
    id: String,
    title: String,
    names: BTreeMap<String, String>,
    started: u64,
    last: u64,
    rounds: u32,
    draws: u32,
    matches: u32,
    wins: BTreeMap<String, u32>,
    match_wins: BTreeMap<String, u32>,
}

/// Folds JSON-lines history text into `{sessions, totals}`; `current` marks the running session.
pub fn aggregate(text: &str, current: &str) -> Value {
    let mut order: Vec<String> = vec![];
    let mut map: BTreeMap<String, Sess> = BTreeMap::new();
    for line in text.lines() {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let (Some(id), Some(ev)) = (e["session"].as_str(), e["ev"].as_str()) else { continue };
        let t = e["t"].as_u64().unwrap_or(0);
        if !map.contains_key(id) {
            order.push(id.to_string());
        }
        let s = map.entry(id.to_string()).or_insert_with(|| Sess { id: id.to_string(), started: t, ..Default::default() });
        s.last = s.last.max(t);
        match ev {
            "start" => {
                s.started = t;
                s.title = e["title"].as_str().unwrap_or("").to_string();
                if let Some(o) = e["sides"].as_object() {
                    for (k, n) in o {
                        s.names.insert(k.clone(), n.as_str().unwrap_or(k).to_string());
                    }
                }
            }
            "round" => {
                s.rounds += 1;
                match e["winner"].as_str() {
                    Some(w) => *s.wins.entry(w.to_string()).or_default() += 1,
                    None => s.draws += 1,
                }
            }
            "match" => {
                s.matches += 1;
                if let Some(w) = e["winner"].as_str() {
                    *s.match_wins.entry(w.to_string()).or_default() += 1;
                }
            }
            _ => {}
        }
    }

    // per side name, across sessions
    let mut totals: BTreeMap<String, (u32, u32, u32, u32)> = BTreeMap::new(); // wins, losses, sessions won, sessions lost
    let (mut rounds, mut draws) = (0u32, 0u32);
    let mut sessions: Vec<Value> = vec![];
    for id in order.iter().rev() {
        let s = &map[id];
        let keys: Vec<&String> = s.names.keys().collect();
        let win = |k: &str| s.wins.get(k).copied().unwrap_or(0);
        let mut sides = serde_json::Map::new();
        for k in &keys {
            let other: u32 = keys.iter().filter(|o| o != &k).map(|o| win(o)).sum();
            sides.insert((*k).clone(), json!({"name": s.names[*k], "wins": win(k), "losses": other, "match_wins": s.match_wins.get(*k).copied().unwrap_or(0)}));
        }
        // the session goes to the side that won more rounds
        let best = keys.iter().map(|k| win(k)).max().unwrap_or(0);
        let leaders: Vec<&&String> = keys.iter().filter(|k| win(k) == best).collect();
        let result = if s.rounds == 0 {
            json!(null)
        } else if leaders.len() == 1 {
            json!(leaders[0])
        } else {
            json!("draw")
        };
        for k in &keys {
            let other: u32 = keys.iter().filter(|o| o != &k).map(|o| win(o)).sum();
            let t = totals.entry(s.names[*k].clone()).or_default();
            t.0 += win(k);
            t.1 += other;
            if s.rounds > 0 {
                if leaders.len() == 1 && **leaders[0] == **k {
                    t.2 += 1;
                } else if leaders.len() == 1 {
                    t.3 += 1;
                }
            }
        }
        rounds += s.rounds;
        draws += s.draws;
        sessions.push(json!({
            "id": s.id,
            "title": s.title,
            "started_unix": s.started,
            "secs": s.last.saturating_sub(s.started),
            "current": s.id == current,
            "rounds": s.rounds,
            "draws": s.draws,
            "matches": s.matches,
            "sides": sides,
            "result": result,
        }));
    }
    let total_sessions = sessions.len();
    sessions.truncate(50);
    let by_side: serde_json::Map<String, Value> = totals
        .into_iter()
        .map(|(n, (w, l, sw, sl))| (n, json!({"wins": w, "losses": l, "sessions_won": sw, "sessions_lost": sl})))
        .collect();
    json!({
        "sessions": sessions,
        "totals": {"sessions": total_sessions, "rounds": rounds, "draws": draws, "by_side": by_side},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_fold_into_wins_losses_and_session_results() {
        let text = r#"
{"t":100,"session":"s1","ev":"start","title":"DEMO","sides":{"A":"RAKYAT","B":"PEMERINTAH"}}
{"t":160,"session":"s1","ev":"round","round":1,"winner":"A"}
{"t":220,"session":"s1","ev":"round","round":2,"winner":"A"}
{"t":280,"session":"s1","ev":"round","round":3,"winner":"B"}
{"t":300,"session":"s1","ev":"hb"}
not json at all
{"t":1000,"session":"s2","ev":"start","title":"DEMO","sides":{"A":"RAKYAT","B":"PEMERINTAH"}}
{"t":1060,"session":"s2","ev":"round","round":1,"winner":null}
{"t":1120,"session":"s2","ev":"round","round":2,"winner":"B"}
{"t":1180,"session":"s2","ev":"match","winner":"B","rounds":2}
"#;
        let v = aggregate(text, "s2");
        let s = v["sessions"].as_array().unwrap();
        assert_eq!(s.len(), 2);
        // newest first, and the running one is marked
        assert_eq!(s[0]["id"], json!("s2"));
        assert_eq!(s[0]["current"], json!(true));
        assert_eq!(s[0]["draws"], json!(1));
        assert_eq!(s[0]["result"], json!("B"));
        assert_eq!(s[0]["matches"], json!(1));
        assert_eq!(s[1]["rounds"], json!(3));
        assert_eq!(s[1]["secs"], json!(200));
        assert_eq!(s[1]["sides"]["A"]["wins"], json!(2));
        assert_eq!(s[1]["sides"]["A"]["losses"], json!(1));
        assert_eq!(s[1]["result"], json!("A"));
        let t = &v["totals"];
        assert_eq!(t["sessions"], json!(2));
        assert_eq!(t["rounds"], json!(5));
        assert_eq!(t["by_side"]["RAKYAT"]["wins"], json!(2));
        assert_eq!(t["by_side"]["RAKYAT"]["losses"], json!(2));
        assert_eq!(t["by_side"]["PEMERINTAH"]["sessions_won"], json!(1));
        assert_eq!(t["by_side"]["PEMERINTAH"]["sessions_lost"], json!(1));
    }

    #[test]
    fn the_file_is_appended_and_read_back() {
        let dir = std::env::temp_dir().join(format!("r3d_hist_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = History::open(dir.join("sub/sessions.jsonl"), "T", [("A".into(), "Red".into()), ("B".into(), "Blue".into())]);
        h.append(json!({"ev": "round", "round": 1, "winner": "B"}));
        let again = History::open(dir.join("sub/sessions.jsonl"), "T", [("A".into(), "Red".into()), ("B".into(), "Blue".into())]);
        let v = again.summary();
        assert_eq!(v["enabled"], json!(true));
        assert!(v["sessions"].as_array().unwrap().len() >= 1);
        assert_eq!(v["totals"]["by_side"]["Blue"]["wins"], json!(1));
    }
}

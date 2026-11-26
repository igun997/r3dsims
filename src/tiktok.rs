//! TikTok LIVE connector built into `r3dsims live`: give it a username and it waits until that account goes live,
//! joins the room, turns chat, gifts, likes, joins, follows and shares into the same events a webhook would carry,
//! hands them to the simulation, and starts over by itself when the stream ends, drops or the account goes live again.
//!
//! It reads TikTok's private web protocol through the vendored `tiktok-live` crate (no login, no signing service). That
//! is not an official API: TikTok can change it, and it may refuse datacenter IPs (use a proxy then).
//!
//! `TikTok` is a handle: `connect` / `disconnect` change the account while running (the dashboard does that), `snapshot`
//! returns the status and the recent events for the dashboard. The work happens on one thread with its own runtime.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tiktok_live::errors::TikTokLiveError;
use tiktok_live::http::api::{FetchParams, fetch_room_id};
use tiktok_live::structs::TikTokLiveEvent;
use tiktok_live::TikTokLive;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::war::Event;

/// Where the simulation takes events from: called with a batch every few hundred milliseconds.
pub type Sink = Arc<dyn Fn(Vec<Event>) + Send + Sync>;

#[derive(Clone, Debug, Default)]
pub struct TikTokOpts {
    /// account to follow from the start (None: wait for `connect`)
    pub username: Option<String>,
    /// `http://`, `https://`, `socks5://` or `socks5h://`, optional `user:pass@`
    pub proxy: Option<String>,
    /// session cookies (only needed for 18+ rooms)
    pub cookies: Option<String>,
    /// seconds between "is it live yet?" checks while the account is offline
    pub poll_secs: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    pub comment: u64,
    pub gift: u64,
    pub like: u64,
    pub join: u64,
    pub follow: u64,
    pub share: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// off | waiting (account not live yet) | connecting | live | reconnecting | error
    pub state: String,
    pub username: String,
    pub room_id: String,
    /// unix time the current room was joined (0 = not joined)
    pub since_unix: u64,
    pub viewers: i64,
    pub likes: i64,
    pub events_total: u64,
    pub events_per_min: u32,
    /// events dropped by the flood guard
    pub dropped: u64,
    pub counts: Counts,
    pub error: String,
    pub hint: String,
    pub proxy: bool,
}

impl Status {
    fn new(proxy: bool) -> Status {
        Status {
            state: "off".into(),
            username: String::new(),
            room_id: String::new(),
            since_unix: 0,
            viewers: 0,
            likes: 0,
            events_total: 0,
            events_per_min: 0,
            dropped: 0,
            counts: Counts::default(),
            error: String::new(),
            hint: String::new(),
            proxy,
        }
    }
}

enum Cmd {
    Connect(String),
    Disconnect,
}

struct Inner {
    status: Mutex<Status>,
    feed: Mutex<VecDeque<Value>>,
    stamps: Mutex<VecDeque<Instant>>,
}

impl Inner {
    fn set(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    fn state(&self, state: &str, error: &str, hint: &str) {
        self.set(|s| {
            s.state = state.into();
            s.error = error.into();
            s.hint = hint.into();
            if state != "live" && state != "reconnecting" {
                s.since_unix = 0;
                s.viewers = 0;
            }
        });
    }

    /// Records events that are about to reach the simulation.
    fn record(&self, events: &[Event]) {
        let now = Instant::now();
        let t = unix_now();
        if let Ok(mut st) = self.stamps.lock() {
            for _ in events {
                st.push_back(now);
            }
            while st.front().is_some_and(|x| now.duration_since(*x) > Duration::from_secs(60)) {
                st.pop_front();
            }
            let n = st.len() as u32;
            drop(st);
            self.set(|s| s.events_per_min = n);
        }
        self.set(|s| {
            for e in events {
                s.events_total += 1;
                match e.kind.as_str() {
                    "comment" => s.counts.comment += 1,
                    "gift" => s.counts.gift += 1,
                    "like" => s.counts.like += e.count as u64,
                    "join" => s.counts.join += 1,
                    "follow" => s.counts.follow += 1,
                    "share" => s.counts.share += 1,
                    _ => {}
                }
            }
        });
        if let Ok(mut feed) = self.feed.lock() {
            // joins are counted but not listed: in a big room they arrive many per second and bury the chat and the gifts
            for e in events.iter().filter(|e| e.kind != "join") {
                let text = match e.kind.as_str() {
                    "comment" => e.text.clone(),
                    "gift" => format!("{} x{}", e.gift, e.count),
                    "like" => format!("x{}", e.count),
                    _ => String::new(),
                };
                feed.push_back(json!({"t": t, "kind": e.kind, "user": e.user, "text": text}));
            }
            while feed.len() > 80 {
                feed.pop_front();
            }
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `@name`, `name`, or a profile / live URL: the bare account name.
pub fn clean_username(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(i) = s.find("tiktok.com/") {
        s = &s[i + "tiktok.com/".len()..];
    }
    let s = s.trim_start_matches('@');
    s.split(['/', '?', '#', ' ']).next().unwrap_or("").trim_start_matches('@').to_string()
}

/// Handle to the connector thread.
pub struct TikTok {
    inner: Arc<Inner>,
    cmd: UnboundedSender<Cmd>,
}

impl TikTok {
    pub fn start(opts: TikTokOpts, sink: Sink) -> TikTok {
        let inner = Arc::new(Inner { status: Mutex::new(Status::new(opts.proxy.is_some())), feed: Mutex::new(VecDeque::new()), stamps: Mutex::new(VecDeque::new()) });
        let (cmd, rx) = unbounded_channel();
        let worker = inner.clone();
        std::thread::Builder::new()
            .name("tiktok".into())
            .spawn(move || match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt.block_on(supervisor(worker, opts, sink, rx)),
                Err(e) => worker.state("error", &format!("cannot start the connector runtime: {e}"), ""),
            })
            .ok();
        TikTok { inner, cmd }
    }

    /// Follow this account (replaces the current one). Accepts `@name`, `name` or a TikTok URL.
    pub fn connect(&self, username: &str) -> Result<(), String> {
        let u = clean_username(username);
        if u.is_empty() {
            return Err("give a TikTok username".into());
        }
        self.cmd.send(Cmd::Connect(u)).map_err(|_| "the TikTok connector is not running".to_string())
    }

    pub fn disconnect(&self) {
        let _ = self.cmd.send(Cmd::Disconnect);
    }

    pub fn status(&self) -> Status {
        self.inner.status.lock().map(|s| s.clone()).unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Status and the most recent events, for the dashboard.
    pub fn snapshot(&self) -> Value {
        let feed: Vec<Value> = self.inner.feed.lock().map(|f| f.iter().rev().take(40).cloned().collect()).unwrap_or_default();
        json!({"status": self.status(), "feed": feed})
    }
}

// ---------------------------------------------------------------- event mapping

/// What one TikTok event means for the simulation and the dashboard.
#[derive(Debug, PartialEq)]
pub enum Out {
    Event(Event),
    Viewers(i64),
    TotalLikes(i64),
    Ended,
}

struct Window {
    start: Instant,
    n: u32,
}

impl Window {
    fn allow(&mut self, now: Instant, cap: u32) -> bool {
        if now.duration_since(self.start) >= Duration::from_secs(1) {
            self.start = now;
            self.n = 0;
        }
        self.n += 1;
        self.n <= cap
    }
}

/// Turns TikTok events into simulation events: gift streaks are reported once when they end, likes of the same viewer
/// are summed until `flush_likes`, repeated messages are dropped, and chat / join floods are capped.
pub struct Mapper {
    likes: HashMap<String, u32>,
    seen: VecDeque<i64>,
    chat: Window,
    join: Window,
    pub dropped: u64,
}

const CHAT_PER_SEC: u32 = 25;
const JOIN_PER_SEC: u32 = 6;

impl Mapper {
    pub fn new(now: Instant) -> Mapper {
        Mapper { likes: HashMap::new(), seen: VecDeque::new(), chat: Window { start: now, n: 0 }, join: Window { start: now, n: 0 }, dropped: 0 }
    }

    fn fresh(&mut self, id: i64) -> bool {
        if id == 0 {
            return true;
        }
        if self.seen.contains(&id) {
            return false;
        }
        self.seen.push_back(id);
        if self.seen.len() > 512 {
            self.seen.pop_front();
        }
        true
    }

    pub fn map(&mut self, ev: &TikTokLiveEvent, now: Instant) -> Vec<Out> {
        let name = |u: &Option<tiktok_live::structs::proto::user::UserIdentity>| u.as_ref().map(|u| u.nickname.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "viewer".to_string());
        let id_of = |c: &Option<tiktok_live::structs::proto::types::CommonMessageData>| c.as_ref().map_or(0, |c| c.msg_id);
        let base = |kind: &str, user: String| Event { kind: kind.into(), user, gift: String::new(), count: 1, diamonds: 0, text: String::new(), side: None, id: None };
        match ev {
            TikTokLiveEvent::Chat(m) => {
                if !self.fresh(id_of(&m.common)) {
                    return vec![];
                }
                if !self.chat.allow(now, CHAT_PER_SEC) {
                    self.dropped += 1;
                    return vec![];
                }
                let mut e = base("comment", name(&m.user));
                e.text = m.comment.clone();
                e.id = Some(format!("tt{}", id_of(&m.common))).filter(|_| id_of(&m.common) != 0);
                vec![Out::Event(e)]
            }
            TikTokLiveEvent::Gift(m) => {
                if !m.is_streak_over() {
                    return vec![];
                }
                let id = id_of(&m.common);
                if !self.fresh(id) {
                    return vec![];
                }
                let Some(g) = m.gift_details.as_ref().filter(|g| !g.gift_name.is_empty()) else { return vec![] };
                let mut e = base("gift", name(&m.user));
                e.gift = g.gift_name.clone();
                e.count = m.repeat_count.max(1) as u32;
                e.diamonds = g.diamond_count.max(0) as u32;
                e.id = Some(format!("tt{id}")).filter(|_| id != 0);
                vec![Out::Event(e)]
            }
            TikTokLiveEvent::Like(m) => {
                if m.like_count > 0 {
                    *self.likes.entry(name(&m.user)).or_default() += m.like_count as u32;
                }
                vec![Out::TotalLikes(m.total_like_count)]
            }
            TikTokLiveEvent::Join(m) => {
                if !self.join.allow(now, JOIN_PER_SEC) {
                    self.dropped += 1;
                    return vec![];
                }
                vec![Out::Event(base("join", name(&m.user)))]
            }
            TikTokLiveEvent::Follow(m) => vec![Out::Event(base("follow", name(&m.user)))],
            TikTokLiveEvent::Share(m) => vec![Out::Event(base("share", name(&m.user)))],
            TikTokLiveEvent::RoomUserSeq(m) => vec![Out::Viewers(m.viewer_count)],
            TikTokLiveEvent::LiveEnded(_) => vec![Out::Ended],
            _ => vec![],
        }
    }

    /// The likes collected since the last call, one event per viewer.
    pub fn flush_likes(&mut self) -> Vec<Event> {
        let mut v: Vec<(String, u32)> = self.likes.drain().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.into_iter().map(|(user, n)| Event { kind: "like".into(), user, gift: String::new(), count: n, diamonds: 0, text: String::new(), side: None, id: None }).collect()
    }
}

// ---------------------------------------------------------------- the connector thread

enum Flow {
    /// look again after this long
    Retry(Duration),
    /// follow another account (None = stop)
    Switch(Option<String>),
    Quit,
}

fn from_cmd(c: Option<Cmd>) -> Flow {
    match c {
        Some(Cmd::Connect(u)) => Flow::Switch(Some(u)),
        Some(Cmd::Disconnect) => Flow::Switch(None),
        None => Flow::Quit,
    }
}

async fn supervisor(inner: Arc<Inner>, opts: TikTokOpts, sink: Sink, mut rx: UnboundedReceiver<Cmd>) {
    let mut want = opts.username.as_deref().map(clean_username).filter(|u| !u.is_empty());
    loop {
        let Some(user) = want.clone() else {
            inner.state("off", "", "");
            inner.set(|s| s.username.clear());
            match from_cmd(rx.recv().await) {
                Flow::Switch(u) => want = u,
                Flow::Quit => return,
                Flow::Retry(_) => {}
            }
            continue;
        };
        inner.set(|s| {
            if s.username != user {
                s.username = user.clone();
                s.counts = Counts::default();
                s.events_total = 0;
                s.likes = 0;
                s.dropped = 0;
            }
        });
        let flow = follow(&inner, &opts, &sink, &user, &mut rx).await;
        let flow = match flow {
            Flow::Retry(wait) => {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => Flow::Retry(wait),
                    c = rx.recv() => from_cmd(c),
                }
            }
            other => other,
        };
        match flow {
            Flow::Switch(u) => want = u,
            Flow::Quit => return,
            Flow::Retry(_) => {}
        }
    }
}

fn explain(e: &TikTokLiveError) -> (String, String) {
    let hint = match e {
        TikTokLiveError::TikTokBlocked(_) => "TikTok menolak IP atau sidik jari ini (biasanya IP datacenter). Pakai --tiktok-proxy (socks5/http) atau jalankan dari jaringan rumah.",
        TikTokLiveError::UserNotFound(_) => "Akun tidak ditemukan: periksa ejaan username.",
        TikTokLiveError::AgeRestricted(_) | TikTokLiveError::SessionRequired(_) => "Room ini butuh cookie sesi: pakai --tiktok-cookies.",
        _ => "",
    };
    (e.to_string(), hint.to_string())
}

/// One pass: wait until the account is live, follow the room until it ends, report why it stopped.
async fn follow(inner: &Arc<Inner>, opts: &TikTokOpts, sink: &Sink, user: &str, rx: &mut UnboundedReceiver<Cmd>) -> Flow {
    let poll = Duration::from_secs(opts.poll_secs.clamp(5, 600));
    inner.state("waiting", "", "");
    let params = FetchParams { timeout: Duration::from_secs(12), proxy: opts.proxy.as_deref(), cookies: opts.cookies.as_deref(), ..Default::default() };
    let looked = tokio::select! {
        r = fetch_room_id(user, params) => r,
        c = rx.recv() => return from_cmd(c),
    };
    let room = match looked {
        Ok(r) => r,
        Err(TikTokLiveError::HostNotOnline(_)) => return Flow::Retry(poll),
        Err(e) => {
            let (msg, hint) = explain(&e);
            inner.state("error", &msg, &hint);
            return Flow::Retry(if matches!(e, TikTokLiveError::TikTokBlocked(_)) { Duration::from_secs(60) } else { poll.max(Duration::from_secs(20)) });
        }
    };

    inner.state("connecting", "", "");
    let mut builder = TikTokLive::builder(user).max_retries(8).language("en");
    if let Some(p) = &opts.proxy {
        builder = builder.proxy(p.clone());
    }
    if let Some(c) = &opts.cookies {
        builder = builder.cookies(c.clone());
    }
    let connected = tokio::select! {
        r = builder.connect() => r,
        c = rx.recv() => return from_cmd(c),
    };
    let mut stream = match connected {
        Ok(s) => s,
        Err(e) => {
            let (msg, hint) = explain(&e);
            inner.state("error", &msg, &hint);
            return Flow::Retry(Duration::from_secs(20));
        }
    };

    let mut mapper = Mapper::new(Instant::now());
    let mut batch: Vec<Event> = vec![];
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let flush = |mapper: &mut Mapper, batch: &mut Vec<Event>| {
        batch.extend(mapper.flush_likes());
        if !batch.is_empty() {
            inner.record(batch);
            sink(std::mem::take(batch));
        }
        let dropped = mapper.dropped;
        inner.set(|s| s.dropped = dropped);
    };
    let room_id = room.room_id.clone();
    let flow = loop {
        tokio::select! {
            c = rx.recv() => {
                match c {
                    Some(Cmd::Connect(u)) if u == user => {}
                    other => break from_cmd(other),
                }
            }
            _ = tick.tick() => flush(&mut mapper, &mut batch),
            ev = stream.next_event() => {
                let ev = match ev {
                    Ok(e) => e,
                    Err(_) => break Flow::Retry(Duration::from_secs(5)),
                };
                match &ev {
                    TikTokLiveEvent::Connected { .. } => {
                        let id = room_id.clone();
                        inner.set(|s| {
                            s.state = "live".into();
                            s.room_id = id;
                            s.since_unix = unix_now();
                            s.error.clear();
                            s.hint.clear();
                        });
                        continue;
                    }
                    TikTokLiveEvent::Reconnecting { attempt, max_retries, .. } => {
                        let note = format!("sambungan putus, mencoba lagi {attempt}/{max_retries}");
                        inner.set(|s| {
                            s.state = "reconnecting".into();
                            s.error = note;
                        });
                        continue;
                    }
                    TikTokLiveEvent::Disconnected => break Flow::Retry(Duration::from_secs(5)),
                    _ => {}
                }
                inner.set(|s| {
                    if s.state != "live" {
                        s.state = "live".into();
                        s.error.clear();
                    }
                });
                let mut ended = false;
                for out in mapper.map(&ev, Instant::now()) {
                    match out {
                        Out::Event(e) => batch.push(e),
                        Out::Viewers(n) => inner.set(|s| s.viewers = n),
                        Out::TotalLikes(n) => inner.set(|s| s.likes = n),
                        Out::Ended => ended = true,
                    }
                }
                if batch.len() >= 40 {
                    flush(&mut mapper, &mut batch);
                }
                if ended {
                    flush(&mut mapper, &mut batch);
                    break Flow::Retry(Duration::from_secs(10));
                }
            }
        }
    };
    flush(&mut mapper, &mut batch);
    if matches!(flow, Flow::Retry(_)) {
        inner.state("waiting", "", "");
    }
    flow
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiktok_live::structs::proto::gift_types::GiftDetails;
    use tiktok_live::structs::proto::messages::*;
    use tiktok_live::structs::proto::types::CommonMessageData;
    use tiktok_live::structs::proto::user::UserIdentity;

    fn user(n: &str) -> Option<UserIdentity> {
        Some(UserIdentity { nickname: n.into(), ..Default::default() })
    }

    fn common(id: i64) -> Option<CommonMessageData> {
        Some(CommonMessageData { msg_id: id, ..Default::default() })
    }

    fn chat(id: i64, who: &str, text: &str) -> TikTokLiveEvent {
        TikTokLiveEvent::Chat(WebcastChatMessage { common: common(id), user: user(who), comment: text.into(), ..Default::default() })
    }

    fn gift(id: i64, who: &str, name: &str, combo: bool, end: bool, repeat: i32, diamonds: i32) -> TikTokLiveEvent {
        TikTokLiveEvent::Gift(WebcastGiftMessage {
            common: common(id),
            user: user(who),
            repeat_count: repeat,
            repeat_end: i32::from(end),
            gift_details: Some(GiftDetails { gift_name: name.into(), gift_type: i32::from(combo), diamond_count: diamonds, ..Default::default() }),
            ..Default::default()
        })
    }

    fn events(v: Vec<Out>) -> Vec<Event> {
        v.into_iter().filter_map(|o| if let Out::Event(e) = o { Some(e) } else { None }).collect()
    }

    #[test]
    fn chat_becomes_a_comment_with_a_stable_id_and_repeats_are_dropped() {
        let now = Instant::now();
        let mut m = Mapper::new(now);
        let e = events(m.map(&chat(7, "Budi", "halo"), now));
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].kind.as_str(), e[0].user.as_str(), e[0].text.as_str(), e[0].id.as_deref()), ("comment", "Budi", "halo", Some("tt7")));
        assert!(events(m.map(&chat(7, "Budi", "halo"), now)).is_empty(), "the same message id arrives once");
        // a message without an id is never treated as a repeat
        assert_eq!(events(m.map(&chat(0, "Ani", "a"), now)).len() + events(m.map(&chat(0, "Ani", "a"), now)).len(), 2);
    }

    #[test]
    fn a_gift_streak_is_one_event_when_it_ends_with_the_final_count() {
        let now = Instant::now();
        let mut m = Mapper::new(now);
        assert!(events(m.map(&gift(1, "Ani", "Rose", true, false, 1, 1), now)).is_empty());
        assert!(events(m.map(&gift(2, "Ani", "Rose", true, false, 3, 1), now)).is_empty());
        let e = events(m.map(&gift(3, "Ani", "Rose", true, true, 5, 1), now));
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].kind.as_str(), e[0].gift.as_str(), e[0].count, e[0].diamonds), ("gift", "Rose", 5, 1));
        // a gift that cannot be combo-ed is reported at once, even with a zero repeat count
        let e = events(m.map(&gift(4, "Dewi", "Galaxy", false, false, 0, 1000), now));
        assert_eq!((e[0].gift.as_str(), e[0].count, e[0].diamonds), ("Galaxy", 1, 1000));
        // a gift without details cannot be named, so it is skipped
        let nameless = TikTokLiveEvent::Gift(WebcastGiftMessage { common: common(5), user: user("X"), ..Default::default() });
        assert!(events(m.map(&nameless, now)).is_empty());
    }

    #[test]
    fn likes_of_one_viewer_are_summed_until_flushed() {
        let now = Instant::now();
        let mut m = Mapper::new(now);
        let like = |who: &str, n: i32, total: i64| TikTokLiveEvent::Like(WebcastLikeMessage { user: user(who), like_count: n, total_like_count: total, ..Default::default() });
        for (who, n, total) in [("Budi", 5, 100), ("Budi", 3, 108), ("Budi", 2, 110), ("Ani", 4, 114)] {
            let out = m.map(&like(who, n, total), now);
            assert_eq!(out, vec![Out::TotalLikes(total)], "the running total goes to the dashboard");
        }
        let flushed = m.flush_likes();
        assert_eq!(flushed.iter().map(|e| (e.user.as_str(), e.count)).collect::<Vec<_>>(), vec![("Budi", 10), ("Ani", 4)]);
        assert!(flushed.iter().all(|e| e.kind == "like"));
        assert!(m.flush_likes().is_empty());
    }

    #[test]
    fn a_flood_of_chat_and_joins_is_capped_and_counted() {
        let now = Instant::now();
        let mut m = Mapper::new(now);
        let chats = (1..=100).map(|i| events(m.map(&chat(i, "Spam", "x"), now)).len()).sum::<usize>();
        assert_eq!(chats as u32, CHAT_PER_SEC);
        let join = || TikTokLiveEvent::Join(WebcastMemberMessage { user: user("V"), ..Default::default() });
        let joins = (0..50).map(|_| events(m.map(&join(), now)).len()).sum::<usize>();
        assert_eq!(joins as u32, JOIN_PER_SEC);
        assert_eq!(m.dropped, u64::from(100 - CHAT_PER_SEC + 50 - JOIN_PER_SEC));
        // a second later the budget is back
        assert_eq!(events(m.map(&chat(1000, "Spam", "x"), now + Duration::from_millis(1100))).len(), 1);
    }

    #[test]
    fn follows_shares_viewers_and_the_end_of_the_stream() {
        let now = Instant::now();
        let mut m = Mapper::new(now);
        let social = || WebcastSocialMessage { user: user("Fan"), ..Default::default() };
        assert_eq!(events(m.map(&TikTokLiveEvent::Follow(social()), now))[0].kind, "follow");
        assert_eq!(events(m.map(&TikTokLiveEvent::Share(social()), now))[0].kind, "share");
        // the raw social message also fires; only the convenience events count, or each follow would count twice
        assert!(m.map(&TikTokLiveEvent::Social(social()), now).is_empty());
        let seq = WebcastRoomUserSeqMessage { viewer_count: 321, ..Default::default() };
        assert_eq!(m.map(&TikTokLiveEvent::RoomUserSeq(seq), now), vec![Out::Viewers(321)]);
        assert_eq!(m.map(&TikTokLiveEvent::LiveEnded(WebcastControlMessage::default()), now), vec![Out::Ended]);
    }

    #[test]
    fn usernames_are_cleaned_from_handles_and_urls() {
        assert_eq!(clean_username("  @detikcom "), "detikcom");
        assert_eq!(clean_username("detikcom"), "detikcom");
        assert_eq!(clean_username("https://www.tiktok.com/@detikcom/live?lang=id"), "detikcom");
        assert_eq!(clean_username("tiktok.com/@a.b_c"), "a.b_c");
        assert_eq!(clean_username("   "), "");
    }
}

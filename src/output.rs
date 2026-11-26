//! The video output of `r3dsims live`: frames go to ffmpeg, which pushes them to an RTMP / SRT / UDP target or a file.
//! It can be started, replaced and stopped while the war keeps running (the dashboard does that, because TikTok gives a
//! new server URL and stream key for every LIVE session) and it restarts itself when ffmpeg dies.

use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::record::ffmpeg_available;

/// ffmpeg fed with raw RGB frames through a pipe.
pub struct FfSink {
    child: Child,
    tx: Option<SyncSender<Arc<Vec<u8>>>>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl FfSink {
    pub fn start(target: &str, w: u32, h: u32, fps: u32, bitrate_k: u32) -> Result<FfSink> {
        if !ffmpeg_available() {
            bail!("the video output needs ffmpeg in PATH");
        }
        let (ow, oh) = (w & !1, h & !1);
        let lower = target.to_ascii_lowercase();
        // Streaming platforms (TikTok, YouTube, Twitch) expect an audio track in the ingest: a silent AAC one keeps the
        // stream accepted. Files and stdout stay video only.
        let network = ["rtmp://", "rtmps://", "srt://", "udp://", "tcp://", "rtp://"].iter().any(|s| lower.starts_with(s));
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "warning", "-use_wallclock_as_timestamps", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s"])
            .arg(format!("{w}x{h}"))
            .args(["-i", "-"]);
        if network {
            cmd.args(["-f", "lavfi", "-i", "anullsrc=channel_layout=stereo:sample_rate=44100", "-map", "0:v", "-map", "1:a", "-c:a", "aac", "-b:a", "128k", "-ar", "44100"]);
        }
        cmd.args(["-vf"])
            .arg(format!("scale={ow}:{oh},format=yuv420p"))
            .args(["-r"])
            .arg(fps.to_string())
            .args(["-c:v", "libx264", "-preset", "veryfast", "-tune", "zerolatency", "-pix_fmt", "yuv420p", "-g"])
            .arg((fps * 2).to_string())
            .args(["-b:v"])
            .arg(format!("{bitrate_k}k"))
            .args(["-maxrate"])
            .arg(format!("{bitrate_k}k"))
            .args(["-bufsize"])
            .arg(format!("{}k", bitrate_k * 2));
        if target == "-" {
            cmd.args(["-f", "mpegts", "pipe:1"]);
            cmd.stdout(Stdio::inherit());
        } else if lower.starts_with("rtmp://") || lower.starts_with("rtmps://") {
            cmd.args(["-f", "flv", target]);
            cmd.stdout(Stdio::null());
        } else if lower.starts_with("srt://") || lower.starts_with("udp://") || lower.starts_with("tcp://") || lower.starts_with("rtp://") {
            cmd.args(["-f", "mpegts", target]);
            cmd.stdout(Stdio::null());
        } else {
            cmd.args(["-y", "-movflags", "+faststart", target]);
            cmd.stdout(Stdio::null());
        }
        cmd.stdin(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = cmd.spawn().context("spawn ffmpeg")?;
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("ffmpeg stdin"))?;
        let (tx, rx) = mpsc::sync_channel::<Arc<Vec<u8>>>(3);
        let writer = std::thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                if stdin.write_all(&frame).is_err() {
                    break;
                }
            }
        });
        Ok(FfSink { child, tx: Some(tx), writer: Some(writer) })
    }

    pub fn push(&self, frame: &Arc<Vec<u8>>) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(frame.clone());
        }
    }

    /// `Some(reason)` once ffmpeg has stopped on its own.
    fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(format!("ffmpeg stopped ({status})")),
            Ok(None) => None,
            Err(e) => Some(format!("ffmpeg: {e}")),
        }
    }

    pub fn finish(mut self) {
        drop(self.tx.take());
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        let _ = self.child.wait();
    }

    /// Stops ffmpeg without waiting for the pipe to drain (it may be stuck on a dead network).
    fn kill(mut self) {
        drop(self.tx.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
    }
}

/// `rtmp://host/app/<key>` shown as `rtmp://host/app/…1234`: the stream key is a password.
pub fn mask_target(target: &str) -> String {
    let lower = target.to_ascii_lowercase();
    if target == "-" {
        return "stdout (MPEG-TS)".into();
    }
    if lower.starts_with("rtmp://") || lower.starts_with("rtmps://") {
        let cut = target.split('?').next().unwrap_or(target);
        return match cut.rsplit_once('/') {
            Some((head, key)) if !key.is_empty() && head.contains("://") && head.rsplit_once("://").is_some_and(|(_, rest)| rest.contains('/')) => {
                let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
                format!("{head}/…{tail}")
            }
            _ => cut.to_string(),
        };
    }
    if lower.contains("://") {
        // srt://host:port?streamid=secret -> srt://host:port
        return target.split('?').next().unwrap_or(target).to_string();
    }
    target.to_string()
}

/// Joins the server URL and the stream key the way the platforms show them. Spaces anywhere are dropped (a pasted address
/// often has one after `/rtmp/`), and a whole address pasted in either field works on its own.
pub fn join_target(server: &str, key: &str) -> String {
    let clean = |s: &str| s.split_whitespace().collect::<String>();
    let (server, key) = (clean(server), clean(key));
    if key.is_empty() {
        server
    } else if server.is_empty() {
        key
    } else if key.contains("://") {
        key
    } else {
        format!("{}/{}", server.trim_end_matches('/'), key.trim_start_matches('/'))
    }
}

/// The service an address belongs to, from its host (Instagram and Facebook share `fbcdn.net`; `s_vt=ig` tells them apart).
pub fn platform_of(target: &str) -> &'static str {
    let lower = target.to_ascii_lowercase();
    let host = lower.split("://").nth(1).unwrap_or("").split(['/', '?', ':']).next().unwrap_or("");
    if lower.contains("s_vt=ig") || host.contains("instagram") {
        "instagram"
    } else if host.ends_with("fbcdn.net") || host.ends_with("facebook.com") {
        "facebook"
    } else if host.contains("tiktok") {
        "tiktok"
    } else if host.contains("youtube.com") {
        "youtube"
    } else if host.ends_with("twitch.tv") {
        "twitch"
    } else {
        "custom"
    }
}

/// What is likely to go wrong for this platform with this picture.
pub fn warnings(platform: &str, target: &str, w: u32, h: u32) -> Vec<String> {
    let mut out = vec![];
    let name = match platform {
        "instagram" => "Instagram",
        "tiktok" => "TikTok",
        _ => "",
    };
    if !name.is_empty() {
        if w >= h {
            out.push(format!("{name} memakai gambar vertikal 9:16 tetapi stream ini {w}x{h}: jalankan live dengan --preset live (720x1280)."));
        } else if ((w as f32 / h as f32) - 9.0 / 16.0).abs() > 0.02 {
            out.push(format!("Rasio {w}x{h} bukan 9:16, {name} akan memotong atau memberi bingkai hitam."));
        }
    }
    if matches!(platform, "instagram" | "facebook") && target.to_ascii_lowercase().starts_with("rtmp://") {
        out.push("Instagram dan Facebook hanya menerima rtmps:// (dengan s), bukan rtmp://.".into());
    }
    out
}

pub struct Output {
    w: u32,
    h: u32,
    fps: u32,
    sink: Option<FfSink>,
    /// the destination including the stream key (never leaves this struct)
    target: String,
    bitrate_k: u32,
    /// the operator wants it running: a dead ffmpeg is restarted
    want: bool,
    since: Option<Instant>,
    since_unix: u64,
    state: &'static str,
    error: String,
    restarts: u32,
    recent: VecDeque<Instant>,
    retry_at: Option<Instant>,
    ffmpeg_ok: bool,
}

impl Output {
    pub fn new(w: u32, h: u32, fps: u32) -> Output {
        Output { w, h, fps, sink: None, target: String::new(), bitrate_k: 3500, want: false, since: None, since_unix: 0, state: "off", error: String::new(), restarts: 0, recent: VecDeque::new(), retry_at: None, ffmpeg_ok: ffmpeg_available() }
    }

    pub fn start(&mut self, target: &str, bitrate_k: u32) -> Result<()> {
        let target = target.trim();
        if target.is_empty() {
            bail!("give a target: rtmp://..., srt://..., udp://..., a file, or -");
        }
        self.stop();
        self.target = target.to_string();
        self.bitrate_k = bitrate_k.clamp(300, 20_000);
        self.want = true;
        self.restarts = 0;
        self.recent.clear();
        self.launch()
    }

    fn launch(&mut self) -> Result<()> {
        match FfSink::start(&self.target, self.w, self.h, self.fps, self.bitrate_k) {
            Ok(s) => {
                self.sink = Some(s);
                self.state = "streaming";
                self.error.clear();
                self.since = Some(Instant::now());
                self.since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                self.retry_at = None;
                Ok(())
            }
            Err(e) => {
                self.state = "error";
                self.error = format!("{e:#}");
                self.since = None;
                Err(e)
            }
        }
    }

    pub fn stop(&mut self) {
        self.want = false;
        self.retry_at = None;
        if let Some(s) = self.sink.take() {
            s.kill();
        }
        self.state = "off";
        self.error.clear();
        self.since = None;
        self.target.clear();
    }

    /// Stops for good at the end of the run: lets ffmpeg flush a file.
    pub fn finish(&mut self) {
        self.want = false;
        if let Some(s) = self.sink.take() {
            s.finish();
        }
    }

    pub fn push(&self, frame: &Arc<Vec<u8>>) {
        if let Some(s) = &self.sink {
            s.push(frame);
        }
    }

    /// Call every frame: notices a dead ffmpeg and restarts it (at most 6 times a minute, 3 s apart).
    pub fn poll(&mut self) {
        let now = Instant::now();
        if let Some(s) = &mut self.sink {
            if let Some(why) = s.exited() {
                if let Some(dead) = self.sink.take() {
                    dead.kill();
                }
                self.state = "restarting";
                self.error = why;
                self.since = None;
                self.retry_at = Some(now + Duration::from_secs(3));
            }
        }
        if self.want && self.sink.is_none() {
            let Some(at) = self.retry_at else { return };
            if now < at {
                return;
            }
            while self.recent.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) {
                self.recent.pop_front();
            }
            if self.recent.len() >= 6 {
                self.want = false;
                self.state = "error";
                self.error = format!("{} (gave up after 6 restarts in a minute: the key may be used up, start the LIVE again and paste the new key)", self.error);
                self.retry_at = None;
                return;
            }
            self.recent.push_back(now);
            self.restarts += 1;
            let previous = self.error.clone();
            if self.launch().is_err() {
                self.state = "restarting";
                self.error = previous;
                self.retry_at = Some(now + Duration::from_secs(3));
            } else {
                self.error = format!("restarted after: {previous}");
            }
        }
    }

    pub fn status(&self) -> Value {
        let platform = if self.target.is_empty() { "" } else { platform_of(&self.target) };
        let notes = if self.target.is_empty() { vec![] } else { warnings(platform, &self.target, self.w, self.h) };
        json!({
            "state": self.state,
            "active": self.sink.is_some(),
            "target": if self.target.is_empty() { String::new() } else { mask_target(&self.target) },
            "bitrate_k": self.bitrate_k,
            "resolution": format!("{}x{}", self.w & !1, self.h & !1),
            "fps": self.fps,
            "since_unix": self.since_unix,
            "uptime_secs": self.since.map_or(0, |t| t.elapsed().as_secs()),
            "restarts": self.restarts,
            "error": self.error,
            "ffmpeg": self.ffmpeg_ok,
            "platform": platform,
            "warnings": notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_key_is_hidden_in_what_the_dashboard_shows() {
        assert_eq!(mask_target("rtmp://push-rtmp.example.com/live/abcd-1234-efgh"), "rtmp://push-rtmp.example.com/live/…efgh");
        assert_eq!(mask_target("rtmps://a.example.com:443/rtmp/KEY9?expire=1&sign=secret"), "rtmps://a.example.com:443/rtmp/…KEY9");
        assert_eq!(mask_target("srt://host:9000?streamid=secret"), "srt://host:9000");
        assert_eq!(mask_target("-"), "stdout (MPEG-TS)");
        // an address without a key part has nothing to hide
        assert_eq!(mask_target("rtmp://host/live"), "rtmp://host/live");
        assert!(!mask_target("rtmp://h/app/SUPERSECRETKEY").contains("SUPERSECRET"));
    }

    #[test]
    fn server_and_key_are_joined_with_one_slash() {
        assert_eq!(join_target("rtmp://h/app", "key"), "rtmp://h/app/key");
        assert_eq!(join_target(" rtmp://h/app/ ", " /key "), "rtmp://h/app/key");
        assert_eq!(join_target("rtmp://h/app/key", ""), "rtmp://h/app/key");
    }

    #[test]
    fn an_output_that_cannot_start_reports_why_and_stays_off() {
        let mut o = Output::new(64, 64, 10);
        assert!(o.start("   ", 3000).is_err());
        // a path in a directory that does not exist: ffmpeg only opens it when the first frame arrives, then dies,
        // and the output notices
        if ffmpeg_available() {
            let bad = std::env::temp_dir().join("r3d-no-such-dir").join("x").join("o.mp4");
            let _ = o.start(bad.to_str().unwrap_or("x.mp4"), 3000);
            let frame = Arc::new(vec![0u8; 64 * 64 * 3]);
            for _ in 0..400 {
                o.push(&frame);
                std::thread::sleep(Duration::from_millis(25));
                o.poll();
                if o.state != "streaming" {
                    break;
                }
            }
            assert_ne!(o.status()["state"], json!("streaming"), "{}", o.status());
            assert!(!o.status()["error"].as_str().unwrap_or("").is_empty());
            o.stop();
        }
        assert_eq!(o.status()["state"], json!("off"));
        assert_eq!(o.status()["target"], json!(""));
    }
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    /// Shaped like an Instagram Live Producer address; the token is made up.
    const IG_SERVER: &str = "rtmps://edgetee-upload-cgk2-1.xx.fbcdn.net:443/rtmp/";
    const IG_KEY: &str = "10000000000000001?s_bl=1&s_fbp=sin2-2&s_ow=10&s_prp=cgk2-1&s_sw=0&s_tids=1&s_vt=ig&a=FAKETOKEN123";

    #[test]
    fn an_instagram_address_is_joined_recognised_and_its_token_hidden() {
        let target = join_target(IG_SERVER, IG_KEY);
        assert_eq!(target, format!("{IG_SERVER}{IG_KEY}"), "one slash, the query string kept intact");
        assert_eq!(platform_of(&target), "instagram");
        let shown = mask_target(&target);
        assert_eq!(shown, "rtmps://edgetee-upload-cgk2-1.xx.fbcdn.net:443/rtmp/…0001");
        assert!(!shown.contains("FAKETOKEN") && !shown.contains("s_vt"), "{shown}");
        // pasted with a space after /rtmp/, as the address is often copied
        assert_eq!(join_target("rtmps://edgetee-upload-cgk2-1.xx.fbcdn.net:443/rtmp/ ", " 10000000000000001?s_vt=ig&a=X"), "rtmps://edgetee-upload-cgk2-1.xx.fbcdn.net:443/rtmp/10000000000000001?s_vt=ig&a=X");
        assert_eq!(join_target(&format!("{IG_SERVER} {IG_KEY}"), ""), target, "the whole address in the first field");
        assert_eq!(join_target("", &target), target, "the whole address in the key field");
    }

    #[test]
    fn platforms_are_told_apart_by_host() {
        assert_eq!(platform_of("rtmps://live-api-s.facebook.com:443/rtmp/KEY"), "facebook");
        assert_eq!(platform_of("rtmps://a.rtmp.youtube.com/live2/KEY"), "youtube");
        assert_eq!(platform_of("rtmp://live.twitch.tv/app/KEY"), "twitch");
        assert_eq!(platform_of("rtmp://push-rtmp-l6.tiktokcdn.com/third/KEY"), "tiktok");
        assert_eq!(platform_of("rtmp://127.0.0.1:1935/live/test"), "custom");
        assert_eq!(platform_of("-"), "custom");
    }

    #[test]
    fn vertical_platforms_complain_about_a_horizontal_or_odd_picture() {
        let ig = join_target(IG_SERVER, IG_KEY);
        assert!(warnings("instagram", &ig, 720, 1280).is_empty(), "720x1280 is what Instagram wants");
        assert!(warnings("instagram", &ig, 1080, 1920).is_empty());
        let wide = warnings("instagram", &ig, 1280, 720);
        assert!(wide.len() == 1 && wide[0].contains("--preset live"), "{wide:?}");
        assert!(warnings("tiktok", "rtmp://x.tiktokcdn.com/a/k", 1080, 1350).iter().any(|w| w.contains("9:16")));
        assert!(warnings("custom", "rtmp://h/a/k", 1280, 720).is_empty(), "other services take any shape");
        // Instagram and Facebook refuse plain rtmp
        assert!(warnings("instagram", "rtmp://edge.fbcdn.net/rtmp/k?s_vt=ig", 720, 1280).iter().any(|w| w.contains("rtmps")));
        assert!(warnings("facebook", "rtmps://live-api-s.facebook.com:443/rtmp/k", 720, 1280).is_empty());
    }
}

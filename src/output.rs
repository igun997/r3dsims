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
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "warning", "-use_wallclock_as_timestamps", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s"])
            .arg(format!("{w}x{h}"))
            .args(["-i", "-", "-vf"])
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
        let lower = target.to_ascii_lowercase();
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

/// Joins the server URL and the stream key the way TikTok shows them.
pub fn join_target(server: &str, key: &str) -> String {
    let server = server.trim();
    let key = key.trim();
    if key.is_empty() {
        server.to_string()
    } else {
        format!("{}/{}", server.trim_end_matches('/'), key.trim_start_matches('/'))
    }
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

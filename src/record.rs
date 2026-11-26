//! Recording for social content. The renderer produces low-res frames; the recorder upscales with
//! nearest-neighbour so pixels stay crisp, then encodes:
//!   *.mp4 / *.mov / *.webm  -> piped to ffmpeg (H.264 yuv420p + faststart, optional audio track)
//!   *.gif                   -> built-in encoder
//!   *.png                   -> still image (last pushed frame)
//!   a directory path        -> numbered PNG sequence

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

/// (width, height) of the final video for a named social preset.
pub fn preset_size(name: &str) -> Option<(u32, u32)> {
    match name {
        "reel" | "story" | "tiktok" | "shorts" => Some((1080, 1920)),
        "square" => Some((1080, 1080)),
        "feed" | "portrait" => Some((1080, 1350)),
        "landscape" | "youtube" => Some((1920, 1080)),
        "preview" => Some((540, 960)),
        _ => None,
    }
}

pub fn upscale_nearest(src: &[u8], sw: usize, sh: usize, ow: usize, oh: usize) -> Vec<u8> {
    let mut out = vec![0u8; ow * oh * 3];
    let xmap: Vec<usize> = (0..ow).map(|x| (x * sw / ow).min(sw - 1) * 3).collect();
    let mut prev_sy = usize::MAX;
    for y in 0..oh {
        let sy = (y * sh / oh).min(sh - 1);
        let (before, row) = out.split_at_mut(y * ow * 3);
        let row = &mut row[..ow * 3];
        if sy == prev_sy {
            // same source row as the output row above: copy it
            row.copy_from_slice(&before[(y - 1) * ow * 3..y * ow * 3]);
        } else {
            let line = &src[sy * sw * 3..(sy + 1) * sw * 3];
            for (px, &sx) in row.chunks_exact_mut(3).zip(&xmap) {
                px.copy_from_slice(&line[sx..sx + 3]);
            }
        }
        prev_sy = sy;
    }
    out
}

pub struct RecordOpts {
    pub out: PathBuf,
    pub fps: u32,
    pub out_w: u32,
    pub out_h: u32,
    pub src_w: usize,
    pub src_h: usize,
    pub audio: Option<PathBuf>,
    pub crf: u32,
}

enum Sink {
    Ffmpeg { child: Child, stdin: Option<ChildStdin> },
    Gif { enc: gif::Encoder<File> },
    Still { path: PathBuf },
    Seq { dir: PathBuf },
}

pub struct Recorder {
    sink: Sink,
    o: RecordOpts,
    pub frames: u32,
}

pub fn write_png(path: &Path, rgb: &[u8], w: u32, h: u32) -> Result<()> {
    let f = BufWriter::new(File::create(path).with_context(|| format!("create {}", path.display()))?);
    let mut enc = png::Encoder::new(f, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgb)?;
    Ok(())
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

impl Recorder {
    pub fn start(o: RecordOpts) -> Result<Recorder> {
        let ext = o.out.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
        if let Some(parent) = o.out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let sink = match ext.as_deref() {
            Some("mp4" | "mov" | "mkv" | "webm") => {
                if !ffmpeg_available() {
                    bail!("ffmpeg not found in PATH; install it for video output, or record to .gif / .png / a directory");
                }
                let (ow, oh) = (o.out_w & !1, o.out_h & !1);
                let mut cmd = Command::new("ffmpeg");
                cmd.args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s"])
                    .arg(format!("{}x{}", o.src_w, o.src_h))
                    .arg("-framerate")
                    .arg(o.fps.to_string())
                    .args(["-i", "-"]);
                if let Some(a) = &o.audio {
                    cmd.arg("-i").arg(a);
                }
                cmd.arg("-vf").arg(format!("scale={ow}:{oh}:flags=neighbor,format=yuv420p")).arg("-r").arg(o.fps.to_string());
                if ext.as_deref() == Some("webm") {
                    cmd.args(["-c:v", "libvpx-vp9", "-b:v", "0", "-crf"]).arg((o.crf + 8).to_string());
                } else {
                    cmd.args(["-c:v", "libx264", "-preset", "medium", "-crf"])
                        .arg(o.crf.to_string())
                        .args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"]);
                }
                if o.audio.is_some() {
                    cmd.args(["-c:a", "aac", "-b:a", "160k", "-shortest"]);
                }
                cmd.arg(&o.out).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit());
                let mut child = cmd.spawn().context("spawn ffmpeg")?;
                let stdin = child.stdin.take();
                Sink::Ffmpeg { child, stdin }
            }
            Some("gif") => {
                let f = File::create(&o.out).with_context(|| format!("create {}", o.out.display()))?;
                let mut enc = gif::Encoder::new(f, o.out_w as u16, o.out_h as u16, &[])?;
                enc.set_repeat(gif::Repeat::Infinite)?;
                Sink::Gif { enc }
            }
            Some("png") => Sink::Still { path: o.out.clone() },
            Some(other) => bail!("unsupported output extension '.{other}' (use .mp4 .mov .webm .gif .png or a directory)"),
            None => {
                std::fs::create_dir_all(&o.out)?;
                Sink::Seq { dir: o.out.clone() }
            }
        };
        Ok(Recorder { sink, o, frames: 0 })
    }

    /// `rgb` is `src_w * src_h * 3` at render resolution.
    pub fn push(&mut self, rgb: &[u8]) -> Result<()> {
        let (ow, oh) = (self.o.out_w as usize, self.o.out_h as usize);
        match &mut self.sink {
            Sink::Ffmpeg { stdin, .. } => {
                let s = stdin.as_mut().ok_or_else(|| anyhow!("ffmpeg already closed"))?;
                s.write_all(rgb).context("ffmpeg closed its input (check the ffmpeg error above)")?;
            }
            Sink::Gif { enc } => {
                let big = upscale_nearest(rgb, self.o.src_w, self.o.src_h, ow, oh);
                let mut f = gif::Frame::from_rgb_speed(ow as u16, oh as u16, &big, 10);
                f.delay = (100 / self.o.fps.max(1)).max(2) as u16;
                enc.write_frame(&f)?;
            }
            Sink::Still { path } => {
                let big = upscale_nearest(rgb, self.o.src_w, self.o.src_h, ow, oh);
                write_png(path, &big, ow as u32, oh as u32)?;
            }
            Sink::Seq { dir } => {
                let big = upscale_nearest(rgb, self.o.src_w, self.o.src_h, ow, oh);
                write_png(&dir.join(format!("frame_{:05}.png", self.frames)), &big, ow as u32, oh as u32)?;
            }
        }
        self.frames += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        if let Sink::Ffmpeg { child, stdin } = &mut self.sink {
            drop(stdin.take());
            let st = child.wait()?;
            if !st.success() {
                bail!("ffmpeg exited with {st}");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_upscale_doubles_pixels() {
        let src = [1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4];
        let up = upscale_nearest(&src, 2, 2, 4, 4);
        assert_eq!(&up[0..3], &[1, 1, 1]);
        assert_eq!(&up[3..6], &[1, 1, 1]);
        assert_eq!(&up[6..9], &[2, 2, 2]);
        assert_eq!(&up[(4 * 3 + 2) * 3..(4 * 3 + 2) * 3 + 3], &[4, 4, 4]);
    }

    #[test]
    fn presets_known() {
        assert_eq!(preset_size("reel"), Some((1080, 1920)));
        assert!(preset_size("nope").is_none());
    }
}

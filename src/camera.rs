//! Orbit camera + rig. The rig is driven three ways: story steps / agent commands (`CameraCmd`),
//! manual input (viewer), and continuous modes (follow, spin). All of them end up in `CameraRig`.

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    /// degrees; 0 = camera on the +Z side looking toward -Z, 90 = camera on +X side
    pub yaw: f32,
    /// degrees above the horizon
    pub pitch: f32,
    /// eye distance (perspective only)
    pub dist: f32,
    /// ortho: world units visible across the *shorter* frame side
    pub zoom: f32,
    /// vertical fov in degrees (perspective)
    pub fov: f32,
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { target: Vec3::ZERO, yaw: 45.0, pitch: 32.0, dist: 60.0, zoom: 30.0, fov: 40.0, ortho: true }
    }
}

pub struct Proj {
    pub view: Mat4,
    pub proj: Mat4,
    pub near: f32,
    pub far: f32,
    pub ortho: bool,
    pub eye: Vec3,
    pub forward: Vec3,
}

impl Proj {
    pub fn vp(&self) -> Mat4 {
        self.proj * self.view
    }

    /// View-space distance from a [0,1] depth value.
    pub fn linear(&self, z: f32) -> f32 {
        if self.ortho {
            self.near + z * (self.far - self.near)
        } else {
            self.near * self.far / (self.far - z * (self.far - self.near))
        }
    }

    /// Projects to pixel coordinates; None if behind the camera.
    pub fn to_screen(&self, p: Vec3, w: usize, h: usize) -> Option<(f32, f32)> {
        let c = self.vp() * p.extend(1.0);
        if c.w <= 1e-4 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some(((n.x * 0.5 + 0.5) * w as f32, (1.0 - (n.y * 0.5 + 0.5)) * h as f32))
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        let d = if self.ortho { self.dist.max(120.0) } else { self.dist };
        self.target + Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos()) * d
    }

    pub fn proj(&self, aspect: f32) -> Proj {
        let eye = self.eye();
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Y);
        let forward = (self.target - eye).normalize();
        if self.ortho {
            let (hw, hh) = if aspect >= 1.0 {
                (self.zoom * 0.5 * aspect, self.zoom * 0.5)
            } else {
                (self.zoom * 0.5, self.zoom * 0.5 / aspect)
            };
            let (near, far) = (1.0, (eye - self.target).length() + 300.0);
            Proj { view, proj: Mat4::orthographic_rh(-hw, hw, -hh, hh, near, far), near, far, ortho: true, eye, forward }
        } else {
            let (near, far) = (0.3, 800.0);
            Proj {
                view,
                proj: Mat4::perspective_rh(self.fov.to_radians(), aspect, near, far),
                near,
                far,
                ortho: false,
                eye,
                forward,
            }
        }
    }

    fn lerp(a: &Camera, b: &Camera, u: f32) -> Camera {
        Camera {
            target: a.target.lerp(b.target, u),
            yaw: a.yaw + (b.yaw - a.yaw) * u,
            pitch: a.pitch + (b.pitch - a.pitch) * u,
            dist: a.dist + (b.dist - a.dist) * u,
            zoom: a.zoom + (b.zoom - a.zoom) * u,
            fov: a.fov + (b.fov - a.fov) * u,
            ortho: b.ortho,
        }
    }
}

// ---------------------------------------------------------------- commands

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CamParams {
    pub target: Option<[f32; 3]>,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub distance: Option<f32>,
    pub zoom: Option<f32>,
    pub fov: Option<f32>,
    pub ortho: Option<bool>,
    #[serde(default)]
    pub secs: f32,
    pub ease: Option<String>,
}

fn d_height() -> f32 {
    1.0
}

/// Camera events. Same JSON whether it comes from a story step, `--commands`, or stdin in the viewer.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CameraCmd {
    /// Instant jump; unspecified fields are kept.
    Set(CamParams),
    /// Tween to the given fields over `secs`.
    Move(CamParams),
    Orbit {
        #[serde(default)]
        dyaw: f32,
        #[serde(default)]
        dpitch: f32,
        #[serde(default)]
        secs: f32,
        ease: Option<String>,
    },
    /// dx = screen right, dz = away from camera, in world units.
    Pan {
        #[serde(default)]
        dx: f32,
        #[serde(default)]
        dz: f32,
        #[serde(default)]
        secs: f32,
        ease: Option<String>,
    },
    /// factor 2 = twice as close.
    Zoom {
        factor: f32,
        #[serde(default)]
        secs: f32,
        ease: Option<String>,
    },
    LookAt {
        actor: String,
        #[serde(default)]
        secs: f32,
        ease: Option<String>,
        #[serde(default = "d_height")]
        height: f32,
    },
    /// Keep the actor centered; `smooth` = follow speed (1/s, default 5).
    Follow { actor: String, smooth: Option<f32> },
    Unfollow,
    /// Continuous yaw rotation in degrees per second (0 stops).
    Spin { dps: f32 },
    Reset,
}

#[derive(Clone, Copy, Debug)]
pub enum Ease {
    Linear,
    Smooth,
    In,
    Out,
}

impl Ease {
    pub fn parse(s: Option<&str>) -> Ease {
        match s {
            Some("linear") => Ease::Linear,
            Some("in") => Ease::In,
            Some("out") => Ease::Out,
            _ => Ease::Smooth,
        }
    }
    pub fn apply(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Ease::Linear => u,
            Ease::Smooth => u * u * (3.0 - 2.0 * u),
            Ease::In => u * u,
            Ease::Out => 1.0 - (1.0 - u) * (1.0 - u),
        }
    }
}

struct Tween {
    from: Camera,
    to: Camera,
    t: f32,
    dur: f32,
    ease: Ease,
}

pub struct CameraRig {
    pub cam: Camera,
    pub home: Camera,
    tween: Option<Tween>,
    pub follow: Option<(String, f32)>,
    pub spin: f32,
}

fn clamp_pitch(p: f32) -> f32 {
    p.clamp(4.0, 88.0)
}

impl CameraRig {
    pub fn new(cam: Camera) -> Self {
        CameraRig { cam, home: cam, tween: None, follow: None, spin: 0.0 }
    }

    pub fn busy(&self) -> bool {
        self.tween.is_some()
    }

    fn go(&mut self, goal: Camera, secs: f32, ease: Option<&str>) -> f32 {
        if secs <= 0.0 {
            self.cam = goal;
            self.tween = None;
            0.0
        } else {
            self.cam.ortho = goal.ortho;
            self.tween = Some(Tween { from: self.cam, to: goal, t: 0.0, dur: secs, ease: Ease::parse(ease) });
            secs
        }
    }

    fn with_params(mut c: Camera, p: &CamParams) -> Camera {
        if let Some(t) = p.target {
            c.target = Vec3::from(t);
        }
        if let Some(v) = p.yaw {
            c.yaw = v;
        }
        if let Some(v) = p.pitch {
            c.pitch = clamp_pitch(v);
        }
        if let Some(v) = p.distance {
            c.dist = v;
        }
        if let Some(v) = p.zoom {
            c.zoom = v;
        }
        if let Some(v) = p.fov {
            c.fov = v;
        }
        if let Some(v) = p.ortho {
            c.ortho = v;
        }
        c
    }

    /// Applies a command. `actor_pos` is the resolved position for `LookAt`.
    /// Returns the seconds a story step should wait (0 = instant).
    pub fn apply(&mut self, cmd: &CameraCmd, actor_pos: Option<Vec3>) -> f32 {
        let cur = self.cam;
        match cmd {
            CameraCmd::Set(p) => self.go(Self::with_params(cur, p), 0.0, None),
            CameraCmd::Move(p) => self.go(Self::with_params(cur, p), p.secs, p.ease.as_deref()),
            CameraCmd::Orbit { dyaw, dpitch, secs, ease } => {
                let mut g = cur;
                g.yaw += dyaw;
                g.pitch = clamp_pitch(g.pitch + dpitch);
                self.go(g, *secs, ease.as_deref())
            }
            CameraCmd::Pan { dx, dz, secs, ease } => {
                let y = cur.yaw.to_radians();
                let right = Vec3::new(y.cos(), 0.0, -y.sin());
                let fwd = Vec3::new(-y.sin(), 0.0, -y.cos());
                let mut g = cur;
                g.target += right * *dx + fwd * *dz;
                self.follow = None;
                self.go(g, *secs, ease.as_deref())
            }
            CameraCmd::Zoom { factor, secs, ease } => {
                let f = factor.max(0.01);
                let mut g = cur;
                g.zoom /= f;
                g.dist /= f;
                self.go(g, *secs, ease.as_deref())
            }
            CameraCmd::LookAt { secs, ease, height, .. } => match actor_pos {
                Some(p) => {
                    let mut g = cur;
                    g.target = p + Vec3::new(0.0, *height, 0.0);
                    self.follow = None;
                    self.go(g, *secs, ease.as_deref())
                }
                None => 0.0,
            },
            CameraCmd::Follow { actor, smooth } => {
                self.follow = Some((actor.clone(), smooth.unwrap_or(5.0)));
                0.0
            }
            CameraCmd::Unfollow => {
                self.follow = None;
                0.0
            }
            CameraCmd::Spin { dps } => {
                self.spin = *dps;
                0.0
            }
            CameraCmd::Reset => {
                self.follow = None;
                self.spin = 0.0;
                let home = self.home;
                self.go(home, 0.0, None)
            }
        }
    }

    pub fn update(&mut self, dt: f32, follow_pos: Option<Vec3>) {
        if let Some(tw) = &mut self.tween {
            tw.t += dt;
            let u = tw.ease.apply(tw.t / tw.dur);
            self.cam = Camera::lerp(&tw.from, &tw.to, u);
            if tw.t >= tw.dur {
                self.cam = tw.to;
                self.tween = None;
            }
        } else if let (Some(p), Some((_, smooth))) = (follow_pos, &self.follow) {
            let k = 1.0 - (-smooth * dt).exp();
            self.cam.target += (p - self.cam.target) * k;
        }
        self.cam.yaw += self.spin * dt;
    }

    // ---- manual control (viewer)

    pub fn manual_orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.tween = None;
        self.cam.yaw += dyaw;
        self.cam.pitch = clamp_pitch(self.cam.pitch + dpitch);
    }

    /// Pan in the ground plane: dx screen-right, dz away from camera (world units).
    pub fn manual_pan(&mut self, dx: f32, dz: f32) {
        self.tween = None;
        self.follow = None;
        let y = self.cam.yaw.to_radians();
        self.cam.target += Vec3::new(y.cos(), 0.0, -y.sin()) * dx + Vec3::new(-y.sin(), 0.0, -y.cos()) * dz;
    }

    pub fn manual_zoom(&mut self, factor: f32) {
        self.tween = None;
        let f = factor.max(0.01);
        self.cam.zoom = (self.cam.zoom / f).clamp(4.0, 200.0);
        self.cam.dist = (self.cam.dist / f).clamp(4.0, 500.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tween_reaches_goal() {
        let mut rig = CameraRig::new(Camera::default());
        let secs = rig.apply(
            &CameraCmd::Move(CamParams { yaw: Some(135.0), secs: 2.0, ..Default::default() }),
            None,
        );
        assert_eq!(secs, 2.0);
        rig.update(1.0, None);
        assert!(rig.cam.yaw > 45.0 && rig.cam.yaw < 135.0);
        rig.update(1.5, None);
        assert_eq!(rig.cam.yaw, 135.0);
        assert!(!rig.busy());
    }

    #[test]
    fn json_roundtrip() {
        let c: CameraCmd = serde_json::from_str(r#"{"op":"move","yaw":90,"secs":1.5}"#).unwrap();
        assert!(matches!(c, CameraCmd::Move(p) if p.yaw == Some(90.0)));
    }
}

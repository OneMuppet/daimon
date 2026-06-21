//! Minimal column-major linear algebra for the isometric 3-D camera — no glam.
//!
//! Daimon: Smallworld renders its village as real 3-D geometry under an
//! orthographic god-view: a fixed yaw/pitch "storybook isometric". This is just
//! enough math to build the view-projection, invert a cursor back onto the
//! ground (picking), and project a world point to the screen (world-anchored
//! labels). Pure + host-testable. Coordinate frame: sim is 2-D cells `(x, y)`;
//! world is `(x, up, y)` — the sim plane lies in world XZ, height is world Y.

/// A small 3-vector with the handful of ops the camera needs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[allow(clippy::should_implement_trait)]
impl Vec3 {
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    #[inline]
    pub fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    #[inline]
    pub fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    #[inline]
    pub fn scale(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
    #[inline]
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline]
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    #[inline]
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 1e-6 {
            self.scale(1.0 / l)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        }
    }
}

/// 4×4 matrix in COLUMN-MAJOR order (`m[col*4 + row]`): uploads straight to a
/// WGSL `mat4x4<f32>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let (a, b) = (&self.0, &rhs.0);
        let mut m = [0.0f32; 16];
        for c in 0..4 {
            for r in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += a[k * 4 + r] * b[c * 4 + k];
                }
                m[c * 4 + r] = s;
            }
        }
        Mat4(m)
    }

    /// Right-handed orthographic, clip-space z in `[0,1]` (WebGPU convention).
    pub fn orthographic_rh_zo(l: f32, r: f32, b: f32, t: f32, near: f32, far: f32) -> Mat4 {
        let mut m = [0.0f32; 16];
        m[0] = 2.0 / (r - l);
        m[5] = 2.0 / (t - b);
        m[10] = 1.0 / (near - far);
        m[12] = (l + r) / (l - r);
        m[13] = (t + b) / (b - t);
        m[14] = near / (near - far);
        m[15] = 1.0;
        Mat4(m)
    }

    /// Right-handed perspective, clip-space z in `[0,1]` (WebGPU convention).
    /// `fov_y` in radians; `aspect` = width / height.
    pub fn perspective_rh_zo(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fov_y * 0.5).tan();
        let mut m = [0.0f32; 16];
        m[0] = f / aspect;
        m[5] = f;
        m[10] = far / (near - far);
        m[11] = -1.0;
        m[14] = (near * far) / (near - far);
        Mat4(m)
    }

    /// Right-handed look-at view matrix.
    pub fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
        let f = center.sub(eye).normalized();
        let s = f.cross(up).normalized();
        let u = s.cross(f);
        Mat4([
            s.x, u.x, -f.x, 0.0, //
            s.y, u.y, -f.y, 0.0, //
            s.z, u.z, -f.z, 0.0, //
            -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
        ])
    }

    #[inline]
    pub fn to_cols(&self) -> [f32; 16] {
        self.0
    }

    /// Transform a homogeneous point (w = 1); returns clip-space `[x, y, z, w]`.
    pub fn transform_point(&self, p: Vec3) -> [f32; 4] {
        let m = &self.0;
        [
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
            m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15],
        ]
    }
}

// --- the iso rig -----------------------------------------------------------

/// Daimon's storybook-isometric angle (gentler than a hard 45/30 to feel less
/// like a strategy grid and more like a painted diorama).
pub const ISO_YAW_DEG: f32 = 38.0;
pub const ISO_PITCH_DEG: f32 = 33.0;
/// Eye distance along the view ray (orthographic: affects only near/far clip).
pub const CAM_DIST: f32 = 120.0;

/// `(eye, fwd, right, up)` for an iso look-at a ground target. `screen_to_ground`
/// inverts exactly what this builds.
pub fn iso_basis(tx: f32, ty: f32, target_y: f32, yaw: f32) -> (Vec3, Vec3, Vec3, Vec3) {
    let pitch = ISO_PITCH_DEG.to_radians();
    let world_up = Vec3::new(0.0, 1.0, 0.0);
    let dir = Vec3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos());
    let center = Vec3::new(tx, target_y, ty);
    let eye = center.add(dir.scale(CAM_DIST));
    let fwd = dir.scale(-1.0);
    let right = fwd.cross(world_up).normalized();
    let up = right.cross(fwd).normalized();
    (eye, fwd, right, up)
}

/// Orthographic view-projection for the god view. `zoom` is the VERTICAL
/// half-extent in world units; horizontal follows the aspect.
pub fn iso_view_proj(tx: f32, ty: f32, target_y: f32, zoom: f32, aspect: f32, yaw: f32) -> Mat4 {
    let (eye, fwd, _, _) = iso_basis(tx, ty, target_y, yaw);
    let center = eye.add(fwd.scale(CAM_DIST));
    let view = Mat4::look_at_rh(eye, center, Vec3::new(0.0, 1.0, 0.0));
    let proj = Mat4::orthographic_rh_zo(
        -zoom * aspect,
        zoom * aspect,
        -zoom,
        zoom,
        0.1,
        CAM_DIST * 2.0 + 400.0,
    );
    proj.mul(&view)
}

/// Invert a cursor (NDC, x right / y up in [-1,1]) onto the ground plane
/// `y = plane_y`. Orthographic: the ray direction is constant; the origin slides
/// across the view plane. Returns sim coords `(x, y)`.
#[allow(clippy::too_many_arguments)]
pub fn screen_to_ground(
    ndc: [f32; 2],
    tx: f32,
    ty: f32,
    target_y: f32,
    zoom: f32,
    aspect: f32,
    plane_y: f32,
    yaw: f32,
) -> (f32, f32) {
    let (eye, fwd, right, up) = iso_basis(tx, ty, target_y, yaw);
    let origin = eye.add(right.scale(ndc[0] * zoom * aspect)).add(up.scale(ndc[1] * zoom));
    let t = (plane_y - origin.y) / fwd.y;
    let hit = origin.add(fwd.scale(t));
    (hit.x, hit.z)
}

/// The iso billboard axes (unit right + up) for camera-facing particles/glows.
pub fn iso_axes(tx: f32, ty: f32, yaw: f32) -> ([f32; 3], [f32; 3]) {
    let (_, _, right, up) = iso_basis(tx, ty, 0.0, yaw);
    ([right.x, right.y, right.z], [up.x, up.y, up.z])
}

// --- the first-person "drop in" rig ----------------------------------------
//
// A free-roam PERSPECTIVE camera you can walk through the world in: an eye at
// human height, looking along a yaw/pitch direction with a normal FOV. This is a
// pure render+input alternative to the iso god-view above; it shares the same
// world geometry, just a different view-projection. Coordinate frame matches the
// rest of the file: sim plane in world XZ, height in world Y.

/// Daimon's first-person field of view (vertical), in degrees — a natural ~60°.
pub const FP_FOV_DEG: f32 = 62.0;

/// `(forward, right)` unit world directions for a first-person look at `yaw`/`pitch`
/// (radians). `yaw` sweeps the XZ plane (matching the iso yaw convention); `pitch`
/// tilts up (+) / down (−). `right` is the strafe axis, always level with the
/// ground (no roll). Walking uses the GROUND-projected forward so looking up/down
/// never changes your pace.
pub fn fp_basis(yaw: f32, pitch: f32) -> (Vec3, Vec3) {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let forward = Vec3::new(cy * cp, sp, sy * cp).normalized();
    // right = forward × world_up, but kept perfectly level (computed from yaw only)
    // so strafing never drifts vertically and there is no camera roll.
    let right = Vec3::new(sy, 0.0, -cy).normalized();
    (forward, right)
}

/// Camera-facing billboard axes (unit right + up) for the first-person view, so
/// glows/particles face the walking eye. Built from the look direction rather
/// than the fixed iso angle — the iso version (`iso_axes`) would face the wrong
/// way once you turn around in first person.
pub fn fp_axes(yaw: f32, pitch: f32) -> ([f32; 3], [f32; 3]) {
    let (forward, right) = fp_basis(yaw, pitch);
    let up = right.cross(forward).normalized();
    ([right.x, right.y, right.z], [up.x, up.y, up.z])
}

/// First-person perspective view-projection from an eye position looking along
/// `yaw`/`pitch` with vertical field of view `fov` (radians). Right-handed,
/// WebGPU clip space — uploads to the same `view_proj` slot as the iso path, so
/// `gfx` renders the identical scene from a walk-through camera.
pub fn perspective_view_proj(eye: Vec3, yaw: f32, pitch: f32, fov: f32, aspect: f32) -> Mat4 {
    let (forward, _) = fp_basis(yaw, pitch);
    let center = eye.add(forward);
    let view = Mat4::look_at_rh(eye, center, Vec3::new(0.0, 1.0, 0.0));
    // Near plane tight so you can stand among the figures; far reaches across the
    // whole island and into the surrounding sky/sea so the horizon reads.
    let proj = Mat4::perspective_rh_zo(fov, aspect, 0.05, 600.0);
    proj.mul(&view)
}

/// The ground-plane pan basis: world directions that read as "screen right" and
/// "screen up" when dragging.
pub fn pan_basis(yaw: f32) -> ((f32, f32), (f32, f32)) {
    ((yaw.sin(), -yaw.cos()), (-yaw.cos(), -yaw.sin()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_looks_down() {
        let (_, fwd, right, up) = iso_basis(20.0, 13.0, 1.0, ISO_YAW_DEG.to_radians());
        assert!(fwd.y < -0.4, "the view looks down");
        assert!(right.dot(up).abs() < 1e-5 && right.dot(fwd).abs() < 1e-5, "orthonormal");
        assert!((right.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fp_basis_is_level_and_aims() {
        // looking flat along +x (yaw 0, pitch 0): forward ≈ +x, right level on z.
        let (f, r) = fp_basis(0.0, 0.0);
        assert!(f.x > 0.99 && f.y.abs() < 1e-5, "flat forward points along +x");
        assert!(r.y.abs() < 1e-6, "strafe stays level (no roll)");
        assert!(r.dot(f).abs() < 1e-5, "forward ⟂ right");
        // pitching up raises the forward y but leaves right level.
        let (fu, ru) = fp_basis(0.0, 0.6);
        assert!(fu.y > 0.0 && ru.y.abs() < 1e-6, "pitch up tilts forward, not strafe");
    }

    #[test]
    fn fp_projects_target_ahead_to_centre() {
        // A point straight ahead of the eye projects near screen centre (NDC ~0).
        let eye = Vec3::new(10.0, 1.7, 10.0);
        let (yaw, pitch) = (0.4f32, -0.1f32);
        let (f, _) = fp_basis(yaw, pitch);
        let ahead = eye.add(f.scale(8.0));
        let vp = perspective_view_proj(eye, yaw, pitch, FP_FOV_DEG.to_radians(), 1.6);
        let c = vp.transform_point(ahead);
        assert!(c[3] > 0.0, "ahead point is in front of the camera (w > 0)");
        let ndc = [c[0] / c[3], c[1] / c[3]];
        assert!(ndc[0].abs() < 0.02 && ndc[1].abs() < 0.02, "ahead → centre: {ndc:?}");
    }

    #[test]
    fn picking_roundtrips() {
        let (tx, ty, target_y, zoom, aspect, plane_y) = (12.0, 7.0, 1.0, 16.0, 1.6, 0.5);
        let yaw = ISO_YAW_DEG.to_radians();
        for (px, pz) in [(0.0, 0.0), (15.0, 9.0), (-8.0, 20.0), (30.0, -4.0)] {
            let vp = iso_view_proj(tx, ty, target_y, zoom, aspect, yaw);
            let c = vp.transform_point(Vec3::new(px, plane_y, pz));
            let ndc = [c[0] / c[3], c[1] / c[3]];
            let (hx, hy) = screen_to_ground(ndc, tx, ty, target_y, zoom, aspect, plane_y, yaw);
            assert!((hx - px).abs() < 1e-2 && (hy - pz).abs() < 1e-2, "({px},{pz}) -> ({hx},{hy})");
        }
    }
}

use bytemuck::{Pod, Zeroable};
use serde::Serialize;

pub type Color = [f32; 4];
pub fn rgb(hex: u32) -> Color {
    [
        ((hex >> 16) & 255) as f32 / 255.,
        ((hex >> 8) & 255) as f32 / 255.,
        (hex & 255) as f32 / 255.,
        1.,
    ]
}
pub fn alpha(mut c: Color, a: f32) -> Color {
    c[3] = a;
    c
}
pub const BG: u32 = 0x090e15;
pub const PANEL: u32 = 0x111820;
pub const BORDER: u32 = 0x607384;
pub const TEXT: u32 = 0xf0f4f8;
pub const MUTED: u32 = 0xb3c1cd;
pub const ACCENT: u32 = 0x64ddbb;

pub fn file_background(theme: u8) -> u32 {
    [0xdce3e9, 0x47637b, 0xe4d7c8, 0x6d597b][theme as usize % 4]
}

/// Keep a directory branch recognizable at every zoom level.
pub fn directory_color(path: &str) -> Color {
    let branch = path.split('/').next().unwrap_or("");
    let value = match branch {
        "" | "src" | "lib" | "libs" => 0x58a79c,
        "library" => 0xc7ac66,
        "compiler" => 0x82afe2,
        "scripts" | "tools" => 0x7098c8,
        "test" | "tests" => 0xc7ac66,
        _ => {
            const COLORS: [u32; 7] = [
                0x58a79c, 0x7098c8, 0x88b489, 0xc7ac66, 0xc19570, 0xa091c1, 0xbd91aa,
            ];
            let hash = branch
                .bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
            COLORS[hash as usize % COLORS.len()]
        }
    };
    rgb(value)
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[1] >= self.y && p[0] <= self.x + self.w && p[1] <= self.y + self.h
    }
    pub fn intersects(self, r: Self) -> bool {
        self.x < r.x + r.w && self.x + self.w > r.x && self.y < r.y + r.h && self.y + self.h > r.y
    }
    pub fn inset(self, n: f32) -> Self {
        Self::new(
            self.x + n,
            self.y + n,
            (self.w - 2. * n).max(0.),
            (self.h - 2. * n).max(0.),
        )
    }
    pub fn center(self) -> [f32; 2] {
        [self.x + self.w / 2., self.y + self.h / 2.]
    }
    pub fn translated(self, x: f32, y: f32) -> Self {
        Self::new(self.x + x, self.y + y, self.w, self.h)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: Color,
}
impl Quad {
    pub fn solid(r: Rect, c: Color, radius: f32) -> Self {
        Self {
            rect: [r.x, r.y, r.w, r.h],
            uv: [-1., radius, 0., 0.],
            color: c,
        }
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.rect[0] += x;
        self.rect[1] += y;
    }
}
pub fn rect(out: &mut Vec<Quad>, r: Rect, c: Color, radius: f32) {
    if r.w > 0. && r.h > 0. {
        out.push(Quad::solid(r, c, radius));
    }
}
pub fn outline(out: &mut Vec<Quad>, r: Rect, c: Color, t: f32) {
    rect(out, Rect::new(r.x, r.y, r.w, t), c, 0.);
    rect(out, Rect::new(r.x, r.y + r.h - t, r.w, t), c, 0.);
    rect(out, Rect::new(r.x, r.y, t, r.h), c, 0.);
    rect(out, Rect::new(r.x + r.w - t, r.y, t, r.h), c, 0.);
}

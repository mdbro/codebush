use crate::geometry::{Color, Quad};
use fontdue::{Font, FontSettings, Metrics};
use std::collections::{BTreeSet, HashMap};
const SIZE: f32 = 48.;
#[derive(Clone)]
pub struct Glyph {
    pub uv: [f32; 4],
    pub metrics: Metrics,
}
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub glyphs: HashMap<(bool, char), Glyph>,
    pub missing: Vec<char>,
}
impl Atlas {
    pub fn new(mut chars: BTreeSet<char>) -> Self {
        chars.extend((32..=255).filter_map(char::from_u32));
        chars.extend("→←↳⌘⌂−+×›⌄•…✓□▸▾│─λ".chars());
        let mono = Font::from_bytes(
            include_bytes!("../assets/Code.ttf") as &[u8],
            FontSettings::default(),
        )
        .expect("Embedded code font");
        let ui = Font::from_bytes(
            include_bytes!("../assets/Interface.ttf") as &[u8],
            FontSettings::default(),
        )
        .expect("Embedded interface font");
        let mut fallbacks = vec![ui.clone()];
        let requested: Vec<char> = chars
            .iter()
            .copied()
            .filter(|&c| mono.lookup_glyph_index(c) == 0 && !c.is_control())
            .collect();
        #[cfg(windows)]
        let font_root = std::path::PathBuf::from(
            std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()),
        )
        .join("Fonts");
        #[cfg(not(windows))]
        let font_root = std::path::PathBuf::from("/mnt/c/Windows/Fonts");
        for name in [
            "seguisym.ttf",
            "arial.ttf",
            "msyh.ttc",
            "YuGothM.ttc",
            "malgun.ttf",
            "Nirmala.ttc",
            "LeelawUI.ttf",
            "ebrima.ttf",
            "seguiemj.ttf",
        ] {
            if requested
                .iter()
                .all(|&c| fallbacks.iter().any(|f| f.lookup_glyph_index(c) != 0))
            {
                break;
            }
            if let Ok(bytes) = std::fs::read(font_root.join(name)) {
                if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                    fallbacks.push(font);
                }
            }
        }
        let width = 4096;
        let mut pixels = vec![0u8; (width * 4096) as usize];
        let mut glyphs = HashMap::new();
        let mut x = 2u32;
        let mut y = 2u32;
        let mut row_h = 0u32;
        let mut missing = Vec::new();
        for is_ui in [false, true] {
            let font = if is_ui { &ui } else { &mono };
            for &c in &chars {
                if c.is_control() {
                    continue;
                }
                let font = if font.lookup_glyph_index(c) != 0 {
                    Some(font)
                } else {
                    fallbacks.iter().find(|f| f.lookup_glyph_index(c) != 0)
                };
                let Some(font) = font else {
                    if !is_ui {
                        missing.push(c);
                    }
                    continue;
                };
                let (metrics, bitmap) = font.rasterize(c, SIZE);
                if metrics.width == 0 && !c.is_whitespace() {
                    if !is_ui {
                        missing.push(c);
                    }
                    continue;
                }
                let w = metrics.width as u32;
                let h = metrics.height as u32;
                if x + w + 2 > width {
                    x = 2;
                    y += row_h + 4;
                    row_h = 0;
                }
                while y + h + 2 >= pixels.len() as u32 / width {
                    pixels.resize(pixels.len() * 2, 0);
                }
                for j in 0..h as usize {
                    pixels[(y as usize + j) * width as usize + x as usize
                        ..(y as usize + j) * width as usize + x as usize + w as usize]
                        .copy_from_slice(&bitmap[j * w as usize..(j + 1) * w as usize]);
                }
                glyphs.insert(
                    (is_ui, c),
                    Glyph {
                        uv: [x as f32, y as f32, w as f32, h as f32],
                        metrics,
                    },
                );
                x += w + 4;
                row_h = row_h.max(h);
            }
        }
        let height = (y + row_h + 4).next_power_of_two();
        pixels.truncate((width * height) as usize);
        for g in glyphs.values_mut() {
            g.uv[0] /= width as f32;
            g.uv[1] /= height as f32;
            g.uv[2] /= width as f32;
            g.uv[3] /= height as f32;
        }
        Self {
            width,
            height,
            pixels,
            glyphs,
            missing,
        }
    }
    pub fn advance(&self, c: char, size: f32, ui: bool) -> f32 {
        if !self.glyphs.contains_key(&(ui, c)) {
            return self.advance('M', size, ui) * format!("\\u{{{:X}}}", c as u32).len() as f32;
        }
        self.glyphs
            .get(&(ui, if ui { c } else { 'M' }))
            .or_else(|| self.glyphs.get(&(ui, '�')))
            .or_else(|| self.glyphs.get(&(ui, '?')))
            .map_or(size * 0.6, |g| g.metrics.advance_width * size / SIZE)
    }
    pub fn text_width(&self, text: &str, size: f32, ui: bool) -> f32 {
        text.chars().map(|c| self.advance(c, size, ui)).sum()
    }
    pub fn text(
        &self,
        out: &mut Vec<Quad>,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        ui: bool,
    ) -> f32 {
        let mut pen = x;
        for c in text.chars() {
            if !self.glyphs.contains_key(&(ui, c)) {
                pen += self.text(
                    out,
                    &format!("\\u{{{:X}}}", c as u32),
                    pen,
                    y,
                    size,
                    color,
                    ui,
                );
                continue;
            }
            if let Some(g) = self
                .glyphs
                .get(&(ui, c))
                .or_else(|| self.glyphs.get(&(ui, '?')))
            {
                let m = &g.metrics;
                let s = size / SIZE
                    * if ui {
                        1.
                    } else {
                        (self.glyphs[&(false, 'M')].metrics.advance_width / m.advance_width.max(1.))
                            .min(1.)
                    };
                if m.width > 0 && m.height > 0 {
                    out.push(Quad {
                        rect: [
                            pen + m.xmin as f32 * s,
                            y + size * 0.95 - (m.height as f32 + m.ymin as f32) * s,
                            m.width as f32 * s,
                            m.height as f32 * s,
                        ],
                        uv: g.uv,
                        color,
                    });
                }
                pen += self.advance(c, size, ui);
            }
        }
        pen - x
    }
    pub fn clipped_text(
        &self,
        out: &mut Vec<Quad>,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        max_width: f32,
    ) {
        if self.text_width(text, size, true) <= max_width {
            self.text(out, text, x, y, size, color, true);
            return;
        }
        let ell = self.text_width("…", size, true);
        let mut s = String::new();
        let mut w = 0.;
        for c in text.chars() {
            let a = self.advance(c, size, true);
            if w + a + ell > max_width {
                break;
            }
            s.push(c);
            w += a;
        }
        s.push('…');
        self.text(out, &s, x, y, size, color, true);
    }

    pub fn middle_text(
        &self,
        out: &mut Vec<Quad>,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        max_width: f32,
    ) {
        if self.text_width(text, size, true) <= max_width {
            self.text(out, text, x, y, size, color, true);
            return;
        }
        let chars: Vec<char> = text.chars().collect();
        let mut left = 0;
        let mut right = chars.len();
        let mut width = self.text_width("…", size, true);
        while left < right {
            let use_left = left <= chars.len() - right;
            let c = if use_left {
                chars[left]
            } else {
                chars[right - 1]
            };
            let advance = self.advance(c, size, true);
            if width + advance > max_width {
                break;
            }
            width += advance;
            if use_left {
                left += 1;
            } else {
                right -= 1;
            }
        }
        let label: String = chars[..left]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(chars[right..].iter())
            .collect();
        if width <= max_width {
            self.text(out, &label, x, y, size, color, true);
        }
    }
}

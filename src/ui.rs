use crate::{
    camera::Camera,
    font::Atlas,
    geometry::*,
    model::{Codebase, language_color},
    scene::Scene,
};
use rayon::prelude::*;
use std::{
    collections::BTreeSet,
    sync::{Arc, Weak},
};
pub const TOP: f32 = 64.;
pub const BAR: f32 = 52.;
pub const SIDE: f32 = 270.;
pub const BOTTOM: f32 = 30.;
pub const NAV: f32 = 48.;
#[derive(Clone, Debug)]
pub struct Match {
    pub file: usize,
    pub line: usize,
    pub preview: String,
}
pub fn search(base: &Codebase, query: &str, tests: bool) -> Vec<Match> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<Match> = base
        .files
        .par_iter()
        .enumerate()
        .filter(|(_, f)| tests || !f.is_test)
        .flat_map_iter(|(id, f)| {
            let mut hits = Vec::new();
            if f.path.to_lowercase().contains(&q) {
                hits.push(Match {
                    file: id,
                    line: 0,
                    preview: f.path.clone(),
                });
            }
            for line in &f.lines {
                if !tests && f.hidden_lines.contains(&line.number) {
                    continue;
                }
                let s: String = line.spans.iter().map(|s| s.text.as_str()).collect();
                if s.to_lowercase().contains(&q) {
                    hits.push(Match {
                        file: id,
                        line: line.number,
                        preview: s.trim().to_owned(),
                    });
                }
            }
            hits
        })
        .collect();
    // Exact file navigation wins. Otherwise source occurrences come before
    // incidental path substrings (e.g. `pub` inside `rustc_public`).
    let exact_files: BTreeSet<_> = base
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.path.eq_ignore_ascii_case(&q)
                || f.path
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&q))
        })
        .map(|(id, _)| id)
        .collect();
    hits.sort_unstable_by_key(|m| {
        (
            if m.line == 0 && exact_files.contains(&m.file) {
                0
            } else if m.line > 0 {
                1
            } else {
                2
            },
            m.file,
            m.line,
        )
    });
    hits
}
#[derive(Clone, Debug)]
pub enum Action {
    Mode,
    Tests,
    Fit,
    Zoom(f32),
    Read,
    File(usize),
    Directory(String),
    Collapse(String),
    Search,
    ClearSearch,
    Hit(usize),
    Open,
    Reload,
    Help,
    CloseHelp,
    Minimap,
    Diagnostics,
}
#[derive(Clone)]
pub struct Hitbox {
    pub rect: Rect,
    pub action: Action,
}
pub struct UiState {
    pub tests: bool,
    pub selected: Option<usize>,
    pub query: String,
    pub search_active: bool,
    pub matches: Arc<[Match]>,
    pub active_match: usize,
    pub search_navigated: bool,
    pub scroll: f32,
    pub collapsed: BTreeSet<String>,
    pub hover: Option<usize>,
    pub help: bool,
    pub status: String,
    pub fps: f32,
    pub frame_ms: f32,
    pub gpu_bytes: u64,
    pub gpu_name: String,
    pub gpu_backend: String,
    pub searching: bool,
    pub loading: bool,
    pub hitboxes: Vec<Hitbox>,
    pub map: Rect,
    pub map_world: Rect,
    pub last_tests_ms: f64,
    pub nav_cache: Option<(bool, u64, Vec<usize>)>,
    pub minimap_cache: Option<(bool, [f32; 4], Vec<Quad>)>,
    pub match_cache: Option<(Weak<[Match]>, Vec<usize>, BTreeSet<usize>)>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            tests: true,
            selected: None,
            query: String::new(),
            search_active: false,
            matches: Arc::from([]),
            active_match: 0,
            search_navigated: false,
            scroll: 0.,
            collapsed: BTreeSet::new(),
            hover: None,
            help: false,
            status: String::new(),
            fps: 0.,
            frame_ms: 0.,
            gpu_bytes: 0,
            gpu_name: "RTX 5090".into(),
            gpu_backend: String::new(),
            searching: false,
            loading: false,
            hitboxes: Vec::new(),
            map: Rect::default(),
            map_world: Rect::default(),
            last_tests_ms: 0.,
            nav_cache: None,
            minimap_cache: None,
            match_cache: None,
        }
    }
}
pub fn viewport(size: [f32; 2]) -> Rect {
    Rect::new(
        SIDE,
        TOP + BAR,
        (size[0] - SIDE).max(1.),
        (size[1] - TOP - BAR - BOTTOM - NAV).max(1.),
    )
}
impl UiState {
    pub fn hit(&self, p: [f32; 2]) -> Option<Action> {
        self.hitboxes
            .iter()
            .rev()
            .find(|b| b.rect.contains(p))
            .map(|b| b.action.clone())
    }
    fn button(
        &mut self,
        out: &mut Vec<Quad>,
        atlas: &Atlas,
        r: Rect,
        label: &str,
        action: Action,
        active: bool,
    ) {
        rect(
            out,
            r,
            if active { rgb(0x213b38) } else { rgb(0x1a242f) },
            6.,
        );
        outline(out, r, if active { rgb(0x64a38f) } else { rgb(BORDER) }, 1.);
        let w = atlas.text_width(label, 12., true);
        atlas.text(
            out,
            label,
            r.x + (r.w - w) / 2.,
            r.y + 9.,
            12.,
            if active { rgb(ACCENT) } else { rgb(TEXT) },
            true,
        );
        self.hitboxes.push(Hitbox { rect: r, action });
    }
    pub fn draw(
        &mut self,
        base: &Codebase,
        scene: &Scene,
        atlas: &Atlas,
        camera: Camera,
        size: [f32; 2],
    ) -> Vec<Quad> {
        self.hitboxes.clear();
        let [w, h] = size;
        let mut out = Vec::with_capacity(14000);
        let vp = viewport(size);
        let world_view = camera.view(vp);
        let visible_files = if !self.query.is_empty() || camera.zoom < 0.65 {
            scene.visible_files(world_view)
        } else {
            Vec::new()
        };
        // Screen-space selections preserve a legible outline at every zoom level.
        for (id, c) in [
            (self.hover, alpha(rgb(ACCENT), 0.45)),
            (self.selected, rgb(ACCENT)),
        ] {
            if let Some(f) = id.and_then(|id| scene.file(id)) {
                let p = camera.screen([f.bounds.x, f.bounds.y], vp);
                let r = Rect::new(
                    p[0],
                    p[1],
                    f.bounds.w * camera.zoom,
                    f.bounds.h * camera.zoom,
                );
                if r.intersects(vp) {
                    let clipped = Rect::new(
                        r.x.max(vp.x),
                        r.y.max(vp.y),
                        (r.x + r.w).min(vp.x + vp.w) - r.x.max(vp.x),
                        (r.y + r.h).min(vp.y + vp.h) - r.y.max(vp.y),
                    );
                    outline(&mut out, clipped, rgb(0x071018), 3.5);
                    outline(&mut out, clipped, c, 1.5);
                }
            }
        }
        if !self.query.is_empty() {
            let identity = Arc::downgrade(&self.matches);
            if self
                .match_cache
                .as_ref()
                .is_none_or(|(previous, _, _)| !Weak::ptr_eq(previous, &identity))
            {
                let source = self
                    .matches
                    .iter()
                    .enumerate()
                    .filter(|(_, hit)| hit.line > 0)
                    .map(|(i, _)| i)
                    .collect();
                let files = self
                    .matches
                    .iter()
                    .filter(|hit| hit.line == 0)
                    .map(|hit| hit.file)
                    .collect();
                self.match_cache = Some((identity, source, files));
            }
            let (_, source_hits, path_files) = self.match_cache.as_ref().unwrap();
            for &i in &visible_files {
                let f = &scene.files[i];
                let start = source_hits.partition_point(|&i| self.matches[i].file < f.file);
                if camera.zoom < 0.2 {
                    if path_files.contains(&f.file)
                        || source_hits
                            .get(start)
                            .is_some_and(|&i| self.matches[i].file == f.file)
                    {
                        let p = camera.screen([f.bounds.x, f.bounds.y], vp);
                        let x = p[0].max(vp.x);
                        let y = p[1].max(vp.y);
                        let ex = (p[0] + f.bounds.w * camera.zoom).min(vp.x + vp.w);
                        let ey = (p[1] + f.bounds.h * camera.zoom).min(vp.y + vp.h);
                        // At overview scale, mark the matching file; its full
                        // source remains underneath, with exact lines at reading zoom.
                        outline(
                            &mut out,
                            Rect::new(x, y, ex - x, ey - y),
                            rgb(if f.theme % 2 == 0 { 0x006c54 } else { 0xfff0aa }),
                            2.5,
                        );
                    }
                    continue;
                }
                for hit in source_hits[start..]
                    .iter()
                    .map(|&i| &self.matches[i])
                    .take_while(|hit| hit.file == f.file)
                {
                    if hit.line == 0 {
                        continue;
                    }
                    for row in f
                        .rows
                        .iter()
                        .skip(f.rows.partition_point(|r| r.line < hit.line))
                        .take_while(|r| r.line == hit.line)
                    {
                        let p = camera.screen([row.x - 2. * f.text_scale, row.y], vp);
                        let rr = Rect::new(
                            p[0],
                            p[1],
                            (row.chars.max(6) as f32 * atlas.advance('M', 14., false) + 4.)
                                * camera.zoom
                                * f.text_scale,
                            20. * camera.zoom * f.text_scale,
                        );
                        if rr.intersects(vp)
                            && rr.y >= vp.y
                            && rr.y + rr.h <= vp.y + vp.h
                            && rr.x >= vp.x
                        {
                            outline(
                                &mut out,
                                Rect::new(rr.x, rr.y, rr.w.min(vp.x + vp.w - rr.x), rr.h),
                                rgb(if f.theme % 2 == 0 { 0x006c54 } else { 0xfff0aa }),
                                1.,
                            );
                        }
                    }
                }
            }
        }
        // Keep directory structure legible even when source glyphs become subpixel.
        if camera.zoom < 0.65 {
            let visible_dirs = scene.visible_dirs(world_view);
            let clip = |r: Rect| {
                Rect::new(
                    r.x.max(vp.x),
                    r.y.max(vp.y),
                    (r.x + r.w).min(vp.x + vp.w) - r.x.max(vp.x),
                    (r.y + r.h).min(vp.y + vp.h) - r.y.max(vp.y),
                )
            };
            let mut labels: Vec<Rect> = Vec::new();
            let mut label_quads = Vec::new();
            // Directory names take precedence over tiny file captions. The root
            // is already named in the breadcrumb, leaving room for its children.
            for &i in &visible_dirs {
                let dir = &scene.dirs[i];
                if dir.path.is_empty() {
                    continue;
                }
                let p = camera.screen([dir.bounds.x, dir.bounds.y], vp);
                let sw = dir.bounds.w * camera.zoom;
                if sw > 65. && p[0] >= vp.x && p[1] >= vp.y && p[1] + 20. < vp.y + vp.h {
                    let label = format!("{}/", dir.path.rsplit('/').next().unwrap());
                    let tw = (atlas.text_width(&label, 12., true) + 16.)
                        .min(sw - 4.)
                        .min(vp.x + vp.w - p[0] - 4.);
                    let r = Rect::new(p[0] + 2., p[1] + 2., tw, 19.);
                    if !labels.iter().any(|l| l.intersects(r)) {
                        labels.push(r);
                        rect(&mut label_quads, r, directory_color(&dir.path), 2.);
                        rect(
                            &mut label_quads,
                            Rect::new(r.x, r.y, 2., r.h),
                            directory_color(&dir.path),
                            0.,
                        );
                        atlas.middle_text(
                            &mut label_quads,
                            &label,
                            r.x + 7.,
                            r.y + 3.,
                            12.,
                            rgb(0x07131d),
                            tw - 12.,
                        );
                    }
                }
            }
            for &i in &visible_files {
                let file = &scene.files[i];
                let p = camera.screen([file.bounds.x, file.bounds.y], vp);
                let sw = file.bounds.w * camera.zoom;
                let sh = file.bounds.h * camera.zoom;
                let r = Rect::new(p[0], p[1], sw, sh);
                if sw > 8.
                    && sh > 8.
                    && r.intersects(vp)
                    && self.selected != Some(file.file)
                    && self.hover != Some(file.file)
                {
                    outline(&mut out, clip(r), rgb(0x081019), 1.25);
                }
                if sw > 85. && sh > 35. && p[0] >= vp.x && p[1] >= vp.y && p[1] + 17. < vp.y + vp.h
                {
                    let label = base.files[file.file].path.rsplit('/').next().unwrap();
                    let rw = sw.min(vp.x + vp.w - p[0]);
                    if rw > 50. {
                        let mut lr = Rect::new(p[0] + 1., p[1] + 1., rw - 2., 19.);
                        let end = lr.x + lr.w;
                        for label in &labels {
                            if label.intersects(lr) {
                                lr.x = lr.x.max(label.x + label.w + 3.);
                                lr.w = end - lr.x;
                            }
                        }
                        if lr.w < 60. {
                            continue;
                        }
                        labels.push(lr);
                        rect(&mut label_quads, lr, rgb(0x2c343d), 2.);
                        rect(
                            &mut label_quads,
                            Rect::new(lr.x, lr.y, 2., lr.h),
                            language_color(base.files[file.file].language),
                            0.,
                        );
                        atlas.middle_text(
                            &mut label_quads,
                            label,
                            lr.x + 7.,
                            lr.y + 3.,
                            11.5,
                            rgb(0xedf3f5),
                            lr.w - 13.,
                        );
                    }
                }
            }
            // Group contours sit above leaf borders. A dark gutter keeps the
            // branch color distinct from both light and dark source panels.
            for &i in visible_dirs.iter().rev() {
                let dir = &scene.dirs[i];
                let p = camera.screen([dir.bounds.x, dir.bounds.y], vp);
                let r = Rect::new(
                    p[0],
                    p[1],
                    dir.bounds.w * camera.zoom,
                    dir.bounds.h * camera.zoom,
                );
                if r.w > 65. && r.h > 35. && r.intersects(vp) {
                    let thickness = if dir.depth <= 1 { 2. } else { 1. };
                    outline(&mut out, clip(r), rgb(BG), thickness + 3.);
                    outline(&mut out, clip(r), directory_color(&dir.path), thickness);
                }
            }
            // Captions are the last map layer, above every directory/file outline.
            out.append(&mut label_quads);
        }
        // Top navigation.
        rect(&mut out, Rect::new(0., 0., w, TOP), rgb(0x121920), 0.);
        rect(&mut out, Rect::new(0., TOP - 1., w, 1.), rgb(BORDER), 0.);
        rect(&mut out, Rect::new(22., 19., 26., 26.), rgb(0x27443d), 6.);
        for (x, y) in [(29., 25.), (38., 25.), (38., 35.)] {
            rect(&mut out, Rect::new(x, y, 4., 4.), rgb(ACCENT), 1.);
        }
        rect(&mut out, Rect::new(30., 27., 1., 10.), rgb(ACCENT), 0.);
        rect(&mut out, Rect::new(30., 36., 9., 1.), rgb(ACCENT), 0.);
        atlas.text(&mut out, "CodeBush", 59., 17., 22., rgb(TEXT), true);
        let mode_x = 59. + atlas.text_width("CodeBush", 22., true) + 16.;
        self.button(
            &mut out,
            atlas,
            Rect::new(mode_x, 16., 110., 33.),
            "12D view",
            Action::Mode,
            false,
        );
        let path_x = mode_x + 130.;
        let path_max = (w * 0.31 - path_x - 10.).max(120.);
        atlas.clipped_text(
            &mut out,
            &base.root.display().to_string(),
            path_x,
            24.,
            12.,
            rgb(MUTED),
            path_max,
        );
        let search_x = (w * 0.46).max(480.);
        let search_w = (w * 0.235).clamp(230., 440.);
        let sr = Rect::new(search_x, 16., search_w, 33.);
        rect(&mut out, sr, rgb(0x0c1218), 6.);
        outline(
            &mut out,
            sr,
            if self.search_active {
                rgb(ACCENT)
            } else {
                rgb(BORDER)
            },
            1.,
        );
        let lens = Rect::new(sr.x + 12., sr.y + 10., 10., 10.);
        rect(&mut out, lens, rgb(MUTED), 5.);
        rect(&mut out, lens.inset(1.3), rgb(0x0c1218), 4.);
        rect(
            &mut out,
            Rect::new(sr.x + 21., sr.y + 19., 4., 2.),
            rgb(MUTED),
            1.,
        );
        let value = if self.query.is_empty() {
            "Find a file or search source…"
        } else {
            &self.query
        };
        atlas.clipped_text(
            &mut out,
            value,
            sr.x + 34.,
            sr.y + 8.,
            12.,
            if self.query.is_empty() {
                rgb(MUTED)
            } else {
                rgb(TEXT)
            },
            sr.w - 92.,
        );
        if self.query.is_empty() {
            atlas.text(
                &mut out,
                "Ctrl K",
                sr.x + sr.w - 48.,
                sr.y + 10.,
                10.,
                rgb(MUTED),
                true,
            );
        } else {
            atlas.text(
                &mut out,
                "×",
                sr.x + sr.w - 24.,
                sr.y + 5.,
                18.,
                rgb(MUTED),
                true,
            );
            self.hitboxes.push(Hitbox {
                rect: Rect::new(sr.x + sr.w - 34., sr.y, 34., sr.h),
                action: Action::ClearSearch,
            });
        }
        self.hitboxes.insert(
            0,
            Hitbox {
                rect: sr,
                action: Action::Search,
            },
        );
        self.button(
            &mut out,
            atlas,
            Rect::new(sr.x + sr.w + 16., 16., 104., 33.),
            "Open folder",
            Action::Open,
            false,
        );
        if w > 1400. {
            rect(&mut out, Rect::new(w - 162., 29., 5., 5.), rgb(ACCENT), 2.5);
            atlas.text(&mut out, "RTX 5090", w - 148., 23., 12., rgb(TEXT), true);
            atlas.text(
                &mut out,
                &self.gpu_backend,
                w - 75.,
                25.,
                9.,
                rgb(MUTED),
                true,
            );
        }
        // Canvas breadcrumb and controls.
        rect(
            &mut out,
            Rect::new(SIDE, TOP, w - SIDE, BAR),
            rgb(0x10171f),
            0.,
        );
        rect(
            &mut out,
            Rect::new(SIDE, TOP + BAR - 1., w - SIDE, 1.),
            rgb(BORDER),
            0.,
        );
        atlas.text(
            &mut out,
            "Source atlas",
            SIDE + 24.,
            TOP + 17.,
            15.,
            rgb(TEXT),
            true,
        );
        let crumb = self
            .selected
            .map(|id| base.files[id].path.as_str())
            .unwrap_or(&base.name);
        atlas.text(
            &mut out,
            "/",
            SIDE + 129.,
            TOP + 18.,
            13.,
            rgb(0x4f6072),
            true,
        );
        atlas.clipped_text(
            &mut out,
            crumb,
            SIDE + 146.,
            TOP + 18.,
            13.,
            rgb(MUTED),
            (w - SIDE - 615.).max(40.),
        );
        let toggle = Rect::new(w - 350., TOP + 10., 157., 32.);
        self.button(&mut out, atlas, toggle, "", Action::Tests, self.tests);
        let tr = Rect::new(toggle.x + 12., toggle.y + 10., 22., 12.);
        rect(
            &mut out,
            tr,
            if self.tests {
                rgb(ACCENT)
            } else {
                rgb(0x455263)
            },
            6.,
        );
        rect(
            &mut out,
            Rect::new(tr.x + if self.tests { 12. } else { 2. }, tr.y + 2., 8., 8.),
            rgb(0x11211e),
            4.,
        );
        // Keep toggle label clear of its switch.
        rect(
            &mut out,
            Rect::new(toggle.x + 38., toggle.y + 3., 116., 25.),
            if self.tests {
                rgb(0x213b38)
            } else {
                rgb(0x1a242f)
            },
            0.,
        );
        atlas.text(
            &mut out,
            if self.tests {
                "Tests included"
            } else {
                "Tests hidden"
            },
            toggle.x + 43.,
            toggle.y + 9.,
            12.,
            if self.tests { rgb(ACCENT) } else { rgb(TEXT) },
            true,
        );
        self.button(
            &mut out,
            atlas,
            Rect::new(w - 181., TOP + 10., 88., 32.),
            "Fit all   F",
            Action::Fit,
            false,
        );
        self.button(
            &mut out,
            atlas,
            Rect::new(w - 82., TOP + 10., 58., 32.),
            "?",
            Action::Help,
            false,
        );
        // File tree and search results.
        rect(
            &mut out,
            Rect::new(0., TOP, SIDE, h - TOP - BOTTOM),
            rgb(0x10171e),
            0.,
        );
        rect(
            &mut out,
            Rect::new(SIDE - 1., TOP, 1., h - TOP),
            rgb(BORDER),
            0.,
        );
        atlas.text(
            &mut out,
            if self.query.is_empty() {
                "EXPLORER"
            } else {
                "SEARCH RESULTS"
            },
            20.,
            85.,
            10.,
            rgb(MUTED),
            true,
        );
        atlas.text(
            &mut out,
            &format!("{} files", scene.files.len()),
            SIDE - 80.,
            85.,
            10.,
            rgb(MUTED),
            true,
        );
        let list_bottom = (h - 345.).max(230.);
        let row_start = 114.;
        let row_h = if self.query.is_empty() { 27. } else { 49. };
        let logical;
        if self.query.is_empty() {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.collapsed.hash(&mut hasher);
            scene.nav.len().hash(&mut hasher);
            let key = hasher.finish();
            if self
                .nav_cache
                .as_ref()
                .is_none_or(|(tests, hash, _)| *tests != self.tests || *hash != key)
            {
                let indices = scene
                    .nav
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        !self.collapsed.iter().any(|p| {
                            r.path.starts_with(p) && r.path.as_bytes().get(p.len()) == Some(&b'/')
                        })
                    })
                    .map(|(i, _)| i)
                    .collect();
                self.nav_cache = Some((self.tests, key, indices));
            }
            let indices = &self.nav_cache.as_ref().unwrap().2;
            logical = indices.len() as f32 * row_h;
            self.scroll = self
                .scroll
                .clamp(0., (logical - (list_bottom - row_start)).max(0.));
            let begin = (self.scroll / row_h).floor() as usize;
            let count = ((list_bottom - row_start) / row_h).ceil() as usize + 1;
            for (row_index, &i) in indices.iter().enumerate().skip(begin).take(count) {
                let row = &scene.nav[i];
                let y = row_start + row_index as f32 * row_h - self.scroll;
                if y < row_start || y + row_h > list_bottom {
                    continue;
                }
                let x = 20. + (row.depth.min(9)) as f32 * 15.;
                if let Some(id) = row.file {
                    let selected = self.selected == Some(id);
                    if selected {
                        rect(
                            &mut out,
                            Rect::new(10., y, SIDE - 20., 26.),
                            rgb(0x21352f),
                            4.,
                        );
                        rect(&mut out, Rect::new(10., y + 4., 2., 18.), rgb(ACCENT), 0.);
                    }
                    rect(
                        &mut out,
                        Rect::new(x + 4., y + 11., 5., 5.),
                        language_color(base.files[id].language),
                        1.,
                    );
                    atlas.clipped_text(
                        &mut out,
                        &row.name,
                        x + 19.,
                        y + 5.,
                        12.,
                        if selected { rgb(ACCENT) } else { rgb(0xaabaca) },
                        SIDE - x - 36.,
                    );
                    self.hitboxes.push(Hitbox {
                        rect: Rect::new(10., y, SIDE - 20., row_h),
                        action: Action::File(id),
                    });
                } else {
                    atlas.text(
                        &mut out,
                        if self.collapsed.contains(&row.path) {
                            "▸"
                        } else {
                            "▾"
                        },
                        x,
                        y + 5.,
                        11.,
                        rgb(MUTED),
                        true,
                    );
                    atlas.clipped_text(
                        &mut out,
                        &row.name,
                        x + 19.,
                        y + 5.,
                        12.,
                        rgb(0xc0cfdd),
                        SIDE - x - 42.,
                    );
                    self.hitboxes.push(Hitbox {
                        rect: Rect::new(12., y, x + 7., row_h),
                        action: Action::Collapse(row.path.clone()),
                    });
                    self.hitboxes.push(Hitbox {
                        rect: Rect::new(x + 19., y, SIDE - x - 24., row_h),
                        action: Action::Directory(row.path.clone()),
                    });
                }
            }
        } else {
            atlas.text(
                &mut out,
                &if self.searching {
                    "Searching…".into()
                } else {
                    format!("{} matches", self.matches.len())
                },
                20.,
                110.,
                11.,
                rgb(ACCENT),
                true,
            );
            logical = self.matches.len() as f32 * row_h;
            let begin = (self.scroll / row_h).floor() as usize;
            let count = ((list_bottom - 139.) / row_h).ceil().max(0.) as usize + 1;
            for (i, m) in self.matches.iter().enumerate().skip(begin).take(count) {
                let y = 139. + i as f32 * row_h - self.scroll;
                if y < 139. || y + row_h > list_bottom {
                    continue;
                }
                if self.active_match == i {
                    rect(
                        &mut out,
                        Rect::new(10., y, SIDE - 20., row_h - 3.),
                        rgb(0x21352f),
                        4.,
                    );
                }
                let f = &base.files[m.file];
                let title = if m.line > 0 {
                    format!("{}:{}", f.path.rsplit('/').next().unwrap(), m.line)
                } else {
                    f.path.rsplit('/').next().unwrap().to_owned()
                };
                atlas.clipped_text(&mut out, &title, 20., y + 4., 12., rgb(TEXT), SIDE - 40.);
                atlas.clipped_text(
                    &mut out,
                    &m.preview,
                    20.,
                    y + 23.,
                    10.,
                    rgb(MUTED),
                    SIDE - 40.,
                );
                self.hitboxes.push(Hitbox {
                    rect: Rect::new(10., y, SIDE - 20., row_h),
                    action: Action::Hit(i),
                });
            }
            if self.matches.is_empty() && !self.searching {
                atlas.text(
                    &mut out,
                    "No matches in included source.",
                    20.,
                    151.,
                    11.,
                    rgb(MUTED),
                    true,
                );
                atlas.text(
                    &mut out,
                    "Try a filename or a shorter query.",
                    20.,
                    173.,
                    10.,
                    rgb(MUTED),
                    true,
                );
            }
        }
        let visible = list_bottom - row_start;
        let total = logical + if self.query.is_empty() { 0. } else { 25. };
        self.scroll = self.scroll.clamp(0., (total - visible).max(0.));
        if total > visible {
            rect(
                &mut out,
                Rect::new(
                    SIDE - 5.,
                    row_start + self.scroll / total * visible,
                    2.,
                    (visible / total * visible).max(18.),
                ),
                rgb(0x435363),
                1.,
            );
        }
        // Selection metadata, overview map, and source counts.
        let info_y = (h - 326.).max(250.);
        rect(
            &mut out,
            Rect::new(20., info_y, SIDE - 40., 1.),
            rgb(BORDER),
            0.,
        );
        if let Some(id) = self.selected {
            let f = &base.files[id];
            atlas.clipped_text(
                &mut out,
                &f.path,
                20.,
                info_y + 16.,
                12.,
                rgb(TEXT),
                SIDE - 40.,
            );
            atlas.text(
                &mut out,
                &format!(
                    "{} · {} lines · {}",
                    f.language,
                    scene.file(id).map_or(0, |f| f.count),
                    if f.is_test { "test" } else { "source" }
                ),
                20.,
                info_y + 39.,
                10.,
                rgb(MUTED),
                true,
            );
        } else {
            atlas.text(
                &mut out,
                "Your code, in context.",
                20.,
                info_y + 16.,
                13.,
                rgb(TEXT),
                true,
            );
            atlas.text(
                &mut out,
                "Select a file to explore its source.",
                20.,
                info_y + 39.,
                10.,
                rgb(MUTED),
                true,
            );
        }
        let map_box = Rect::new(20., h - 236., SIDE - 40., 134.);
        rect(&mut out, map_box, rgb(0x0a1016), 6.);
        outline(&mut out, map_box, rgb(BORDER), 1.);
        let factor = ((map_box.w - 16.) / scene.bounds.w).min((map_box.h - 16.) / scene.bounds.h);
        let mw = scene.bounds.w * factor;
        let mh = scene.bounds.h * factor;
        self.map = Rect::new(
            map_box.x + (map_box.w - mw) / 2.,
            map_box.y + (map_box.h - mh) / 2.,
            mw,
            mh,
        );
        self.map_world = scene.bounds;
        let map_key = [self.map.x, self.map.y, self.map.w, self.map.h];
        let map_rect = |b: Rect| {
            Rect::new(
                self.map.x + b.x * factor,
                self.map.y + b.y * factor,
                (b.w * factor - 1.).max(1.),
                (b.h * factor - 1.).max(1.),
            )
        };
        if self
            .minimap_cache
            .as_ref()
            .is_none_or(|(tests, bounds, _)| *tests != self.tests || *bounds != map_key)
        {
            let quads = scene
                .files
                .iter()
                .map(|p| Quad::solid(map_rect(p.bounds), rgb(file_background(p.theme)), 0.))
                .collect();
            self.minimap_cache = Some((self.tests, map_key, quads));
        }
        out.extend_from_slice(&self.minimap_cache.as_ref().unwrap().2);
        if let Some(file) = self.selected.and_then(|id| scene.file(id)) {
            rect(&mut out, map_rect(file.bounds), rgb(ACCENT), 0.);
        }
        let view = camera.view(vp);
        let x = (self.map.x + view.x * factor).clamp(map_box.x, map_box.x + map_box.w);
        let y = (self.map.y + view.y * factor).clamp(map_box.y, map_box.y + map_box.h);
        let ex = (self.map.x + (view.x + view.w) * factor).clamp(map_box.x, map_box.x + map_box.w);
        let ey = (self.map.y + (view.y + view.h) * factor).clamp(map_box.y, map_box.y + map_box.h);
        outline(
            &mut out,
            Rect::new(x, y, ex - x, ey - y),
            alpha(rgb(ACCENT), 0.7),
            1.,
        );
        self.hitboxes.push(Hitbox {
            rect: map_box,
            action: Action::Minimap,
        });
        atlas.text(
            &mut out,
            "REPOSITORY OVERVIEW",
            20.,
            h - 257.,
            9.,
            rgb(MUTED),
            true,
        );
        atlas.text(
            &mut out,
            &format!("{}", scene.files.len()),
            20.,
            h - 83.,
            18.,
            rgb(TEXT),
            true,
        );
        atlas.text(&mut out, "source files", 20., h - 58., 9., rgb(MUTED), true);
        atlas.text(
            &mut out,
            &compact(scene.line_count),
            135.,
            h - 83.,
            18.,
            rgb(TEXT),
            true,
        );
        atlas.text(
            &mut out,
            "lines of code",
            135.,
            h - 58.,
            9.,
            rgb(MUTED),
            true,
        );
        // Navigation has its own strip, so reading lines stay unobscured.
        rect(
            &mut out,
            Rect::new(SIDE, h - BOTTOM - NAV, w - SIDE, NAV),
            rgb(PANEL),
            0.,
        );
        rect(
            &mut out,
            Rect::new(SIDE, h - BOTTOM - NAV, w - SIDE, 1.),
            rgb(BORDER),
            0.,
        );
        let nav = Rect::new(
            SIDE + (w - SIDE - 376.) / 2.,
            h - BOTTOM - NAV + 5.,
            376.,
            39.,
        );
        for (x, label, action) in [
            (nav.x + 1., "−", Action::Zoom(0.8)),
            (nav.x + 91., "+", Action::Zoom(1.25)),
        ] {
            atlas.text(&mut out, label, x + 12., nav.y + 8., 16., rgb(TEXT), true);
            self.hitboxes.push(Hitbox {
                rect: Rect::new(x, nav.y, 35., nav.h),
                action,
            });
        }
        atlas.text(
            &mut out,
            &if camera.zoom < 0.01 {
                format!("{:.2}%", camera.zoom * 100.)
            } else {
                format!("{:.0}%", camera.zoom * 100.)
            },
            nav.x + 43.,
            nav.y + 12.,
            11.,
            rgb(MUTED),
            true,
        );
        rect(
            &mut out,
            Rect::new(nav.x + 131., nav.y + 10., 1., 19.),
            rgb(BORDER),
            0.,
        );
        atlas.text(
            &mut out,
            "Fit all",
            nav.x + 148.,
            nav.y + 11.,
            12.,
            rgb(TEXT),
            true,
        );
        atlas.text(
            &mut out,
            "F",
            nav.x + 193.,
            nav.y + 12.,
            10.,
            rgb(MUTED),
            true,
        );
        self.hitboxes.push(Hitbox {
            rect: Rect::new(nav.x + 133., nav.y, 86., nav.h),
            action: Action::Fit,
        });
        rect(
            &mut out,
            Rect::new(nav.x + 223., nav.y + 10., 1., 19.),
            rgb(BORDER),
            0.,
        );
        atlas.text(
            &mut out,
            "Read file",
            nav.x + 241.,
            nav.y + 11.,
            12.,
            if self.selected.is_some() {
                rgb(ACCENT)
            } else {
                rgb(MUTED)
            },
            true,
        );
        atlas.text(
            &mut out,
            "Enter",
            nav.x + 315.,
            nav.y + 12.,
            10.,
            rgb(MUTED),
            true,
        );
        self.hitboxes.push(Hitbox {
            rect: Rect::new(nav.x + 225., nav.y, 151., nav.h),
            action: Action::Read,
        });
        // Runtime status reports measured values only.
        if !base.stats.warnings.is_empty() {
            self.hitboxes.push(Hitbox {
                rect: Rect::new(0., h - BOTTOM, w - 480., BOTTOM),
                action: Action::Diagnostics,
            });
        }
        rect(
            &mut out,
            Rect::new(0., h - BOTTOM, w, BOTTOM),
            rgb(0x121b23),
            0.,
        );
        rect(&mut out, Rect::new(0., h - BOTTOM, w, 1.), rgb(BORDER), 0.);
        let msg = if self.loading {
            "Loading source into GPU memory…".to_string()
        } else if !self.status.is_empty() {
            self.status.clone()
        } else if !base.stats.warnings.is_empty() {
            format!(
                "{} scan warnings · see load-report.json",
                base.stats.warnings.len()
            )
        } else {
            "Source only   ·   Scroll to zoom   ·   Drag to pan   ·   Double-click to read".into()
        };
        atlas.clipped_text(
            &mut out,
            &msg,
            20.,
            h - 20.,
            10.,
            rgb(MUTED),
            (w - 515.).max(200.),
        );
        let performance = if self.fps > 0. {
            format!(
                "{:.0} fps   ·   {:.2} ms   |   {:.2} GB resident",
                self.fps,
                self.frame_ms,
                self.gpu_bytes as f64 / 1e9
            )
        } else {
            format!(
                "GPU resident   ·   {:.2} GB   |   120 fps target",
                self.gpu_bytes as f64 / 1e9
            )
        };
        let pw = atlas.text_width(&performance, 10., true);
        atlas.text(
            &mut out,
            &performance,
            w - pw - 22.,
            h - 20.,
            10.,
            rgb(MUTED),
            true,
        );
        if scene.files.is_empty() {
            atlas.text(
                &mut out,
                "No source files in this view",
                SIDE + 70.,
                TOP + BAR + 80.,
                26.,
                rgb(TEXT),
                true,
            );
            atlas.text(
                &mut out,
                "Open a code folder, or include tests to show test-only source.",
                SIDE + 70.,
                TOP + BAR + 128.,
                14.,
                rgb(MUTED),
                true,
            );
        }
        if self.help {
            rect(
                &mut out,
                Rect::new(0., 0., w, h),
                alpha(rgb(0x030609), 0.75),
                0.,
            );
            let r = Rect::new(w / 2. - 240., h / 2. - 243., 480., 486.);
            rect(&mut out, r, rgb(0x17222d), 12.);
            outline(&mut out, r, rgb(BORDER), 1.);
            atlas.text(
                &mut out,
                "Find your way around",
                r.x + 30.,
                r.y + 28.,
                23.,
                rgb(TEXT),
                true,
            );
            atlas.text(
                &mut out,
                "Every file is a place. Every line stays on the map.",
                r.x + 30.,
                r.y + 68.,
                12.,
                rgb(MUTED),
                true,
            );
            for (i, (a, b)) in [
                ("Zoom around the pointer", "Mouse wheel"),
                ("Pan the source atlas", "Drag / arrow keys"),
                ("Read the selected file", "Double-click / Enter"),
                ("Fit the whole repository", "F / Home"),
                ("Search files and source", "Ctrl K / Ctrl F"),
                ("Next / previous match", "Enter / Shift Enter"),
                ("Include or hide tests", "T"),
                ("Open / reload folder", "Ctrl O / Ctrl R"),
                ("Fullscreen / close search", "F11 / Escape"),
            ]
            .into_iter()
            .enumerate()
            {
                let y = r.y + 112. + i as f32 * 30.;
                atlas.text(&mut out, a, r.x + 30., y, 12., rgb(TEXT), true);
                let tw = atlas.text_width(b, 11., true);
                atlas.text(
                    &mut out,
                    b,
                    r.x + r.w - 30. - tw,
                    y + 1.,
                    11.,
                    rgb(ACCENT),
                    true,
                );
            }
            self.hitboxes.clear();
            self.button(
                &mut out,
                atlas,
                Rect::new(r.x + 30., r.y + r.h - 62., 420., 34.),
                "Back to source",
                Action::CloseHelp,
                true,
            );
        }
        out
    }
}
fn compact(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1e6)
    } else if n >= 10000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        n.to_string()
    }
}

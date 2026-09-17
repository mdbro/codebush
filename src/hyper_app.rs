//! One native window for the directory tree and 12D reference projections.
use super::*;
use codebush::{
    geometry::*,
    gpu::HyperGpu,
    projection::{self, Matrix, PAIRS, PLANES, Rotation},
    references::Graph,
};

const SIDE: f32 = 310.;
fn viewport(s: [f32; 2]) -> Rect {
    Rect::new(SIDE, 126., (s[0] - SIDE).max(1.), (s[1] - 180.).max(1.))
}
#[derive(Clone, Copy)]
enum Command {
    Mode,
    Plane(usize),
    Previous,
    Next,
    Tests,
    Fit,
    Read,
    Back,
    File(usize),
    Play,
    Scrub(f64),
    Search,
    Help,
    Open,
    Reload,
    Relations,
    Overlaps,
}
struct View {
    plane: usize,
    from: usize,
    rotation: Rotation,
    t: f64,
    playing: bool,
    animating: bool,
    tests: bool,
    selected: Option<usize>,
    relations: bool,
    candidates: Option<Vec<usize>>,
    hover_ids: Vec<usize>,
    pointer: [f32; 2],
    hover_age: f32,
    hover_suppressed: bool,
    query: String,
    editing: bool,
    help: bool,
    scroll: f32,
    hits: Vec<(Rect, Command)>,
    fps: f32,
    status: String,
    camera: Camera,
    target: Camera,
    rotation_bounds: Option<Rect>,
    framing: Option<(Camera, f64)>,
    fitted: bool,
    reading_origin: Option<(Camera, bool)>,
    active: Vec<(usize, f32)>,
    matrix: Matrix,
    weights: [f32; PLANES],
}
impl View {
    fn new(graph: &Graph, tests: bool) -> Self {
        let plane = (0..PLANES)
            .max_by_key(|&p| graph.plane_edges[tests as usize][p].len())
            .unwrap_or(0);
        let r = projection::frame(plane);
        Self {
            plane,
            from: plane,
            rotation: Rotation::new(r, r),
            t: 1.,
            playing: false,
            animating: false,
            tests,
            selected: None,
            relations: false,
            candidates: None,
            hover_ids: vec![],
            pointer: [-1., -1.],
            hover_age: 0.,
            hover_suppressed: false,
            query: String::new(),
            editing: false,
            help: false,
            scroll: 0.,
            hits: vec![],
            fps: 0.,
            status: String::new(),
            camera: Camera::default(),
            target: Camera::default(),
            rotation_bounds: None,
            framing: None,
            fitted: true,
            reading_origin: None,
            active: vec![],
            matrix: r,
            weights: projection::weights(&r),
        }
    }
    fn refresh(&mut self, graph: &Graph) {
        self.matrix = self.rotation.at(self.t);
        self.weights = projection::weights(&self.matrix);
        self.active = graph.active(self.tests as usize, &self.weights);
        if self
            .selected
            .is_some_and(|id| !self.active.iter().any(|(f, _)| *f == id))
        {
            self.selected = None;
        }
    }
    fn fit(&mut self, h: &HyperGpu, size: [f32; 2]) {
        self.reading_origin = None;
        self.fitted = true;
        if self.t < 1. {
            let bounds = h.rotation_bounds(self.tests as usize, &self.rotation);
            self.rotation_bounds = Some(bounds);
            self.target.fit(bounds, viewport(size));
        } else {
            self.fit_current(h, size);
        }
    }
    fn fit_current(&mut self, h: &HyperGpu, size: [f32; 2]) {
        let mut b: Option<Rect> = None;
        for &(id, _) in &self.active {
            let n = h.bounds(self.tests as usize, id, &self.matrix);
            b = Some(if let Some(b) = b {
                Rect::new(
                    b.x.min(n.x),
                    b.y.min(n.y),
                    (b.x + b.w).max(n.x + n.w) - b.x.min(n.x),
                    (b.y + b.h).max(n.y + n.h) - b.y.min(n.y),
                )
            } else {
                n
            });
        }
        self.target.fit(
            b.unwrap_or(Rect::new(-500., -500., 1000., 1000.)),
            viewport(size),
        );
    }
    fn turn(&mut self, p: usize, _graph: &Graph, h: &HyperGpu, size: [f32; 2]) {
        self.from = self.plane;
        self.plane = p;
        self.rotation = Rotation::new(self.matrix, projection::frame(p));
        self.t = 0.;
        self.animating = true;
        self.scroll = 0.;
        self.fit(h, size);
        // Finish the smooth camera move before geometry starts rotating. The fixed
        // swept envelope then contains every annotation throughout the motion.
        self.framing = Some((self.camera, 0.));
    }
    fn manual_camera(&mut self) {
        self.fitted = false;
        self.framing = None;
        self.playing = false;
        self.animating = false;
    }
    fn focus(&mut self, id: usize, h: &HyperGpu, size: [f32; 2], read: bool) {
        if read && self.reading_origin.is_none() {
            self.reading_origin = Some((self.camera, self.fitted));
        }
        self.manual_camera();
        self.selected = Some(id);
        self.hover_age = 0.;
        self.hover_suppressed = true;
        let b = h.bounds(self.tests as usize, id, &self.matrix);
        if read {
            self.target.zoom = 1.15 / h.reading_scale(self.tests as usize, id).max(1e-6);
            let vp = viewport(size);
            self.target.center = [
                b.x + (vp.w / 2. - 34.) / self.target.zoom,
                b.y + (vp.h / 2. - 32.) / self.target.zoom,
            ];
        } else {
            self.target.fit(b, viewport(size));
        }
    }
    fn back_to_links(&mut self) {
        if let Some((camera, fitted)) = self.reading_origin.take() {
            self.manual_camera();
            self.target = camera;
            self.fitted = fitted;
            self.hover_suppressed = true;
        }
    }
    fn select_file(&mut self, id: usize, h: &HyperGpu, size: [f32; 2]) {
        if self.selected != Some(id) {
            self.back_to_links();
        }
        self.selected = Some(id);
        self.relations = true;
        self.scroll = 0.;
        self.hover_suppressed = true;
        let b = h.bounds(self.tests as usize, id, &self.matrix);
        if !b.intersects(self.target.view(viewport(size))) {
            self.fit(h, size);
        }
    }
    fn button(
        &mut self,
        out: &mut Vec<Quad>,
        atlas: &Atlas,
        r: Rect,
        label: &str,
        c: Command,
        on: bool,
    ) {
        rect(out, r, rgb(if on { 0x20453f } else { 0x172331 }), 5.);
        outline(out, r, rgb(if on { 0x57cbb3 } else { BORDER }), 1.);
        atlas.text(
            out,
            label,
            r.x + 12.,
            r.y + 10.,
            12.,
            rgb(if on { ACCENT } else { TEXT }),
            true,
        );
        self.hits.push((r, c));
    }
    fn draw(
        &mut self,
        l: &Loaded,
        graph: &Graph,
        hg: &HyperGpu,
        size: [f32; 2],
        bytes: u64,
    ) -> Vec<Quad> {
        let a = &l.atlas;
        let [w, h] = size;
        let vp = viewport(size);
        let mut out = Vec::with_capacity(8000);
        self.hits.clear();
        self.hover_ids.clear();
        // Captions are 2D annotations at projected anchors. They do not change geometry.
        let selected_screen = self.selected.map(|id| {
            let b = hg.bounds(self.tests as usize, id, &self.matrix);
            let p = self.camera.screen([b.x, b.y], vp);
            Rect::new(p[0], p[1], b.w * self.camera.zoom, b.h * self.camera.zoom)
        });
        let mut neighborhood = std::collections::BTreeSet::new();
        if let Some(id) = self.selected {
            neighborhood.insert(id);
            for e in &graph.edges {
                if (self.tests || !e.test_only)
                    && self.weights[e.plane] > 1e-6
                    && (e.from == id || e.to == id)
                {
                    neighborhood.insert(e.from);
                    neighborhood.insert(e.to);
                }
            }
        }
        // Give selected endpoints first claim on label space, even when their
        // source panels occupy only a few pixels in the whole-repository view.
        let mut annotation_order = Vec::with_capacity(self.active.len());
        for priority in 0..3 {
            annotation_order.extend(self.active.iter().copied().filter(|(id, _)| {
                let rank = if self.selected == Some(*id) {
                    0
                } else if neighborhood.contains(id) {
                    1
                } else {
                    2
                };
                rank == priority
            }));
        }
        let mut occupied: Vec<Rect> = Vec::new();
        let mut captions = Vec::new();
        for (id, weight) in annotation_order {
            let b = hg.bounds(self.tests as usize, id, &self.matrix);
            let p = self.camera.screen([b.x, b.y], vp);
            let r = Rect::new(p[0], p[1], b.w * self.camera.zoom, b.h * self.camera.zoom);
            if !r.intersects(vp) {
                continue;
            }
            if vp.contains(self.pointer) && r.inset(-4.).contains(self.pointer) {
                self.hover_ids.push(id);
            }
            // Annotation contrast is independent of a link's projection weight.
            // Fade only as membership enters/leaves the view, then keep labels legible.
            let fade = ((weight.sqrt() - 0.001) / 0.02).clamp(0., 1.);
            let opacity = fade * fade * (3. - 2. * fade);
            if self.selected != Some(id)
                && r.w > 2.5
                && r.h > 2.5
                && !selected_screen.is_some_and(|b| b.intersects(r))
            {
                let clipped = Rect::new(
                    r.x.max(vp.x),
                    r.y.max(vp.y),
                    (r.x + r.w).min(vp.x + vp.w) - r.x.max(vp.x),
                    (r.y + r.h).min(vp.y + vp.h) - r.y.max(vp.y),
                );
                outline(
                    &mut out,
                    clipped,
                    alpha(directory_color(&l.base.files[id].path), opacity),
                    1.1,
                );
            }
            if self.selected == Some(id) {
                let q = Rect::new(
                    r.x.max(vp.x),
                    r.y.max(vp.y),
                    (r.x + r.w).min(vp.x + vp.w) - r.x.max(vp.x),
                    (r.y + r.h).min(vp.y + vp.h) - r.y.max(vp.y),
                );
                outline(&mut out, q, rgb(ACCENT), 2.);
            }
            let focused = neighborhood.contains(&id);
            if (r.w >= 45. || focused || self.active.len() <= 64)
                && self.camera.zoom * hg.reading_scale(self.tests as usize, id) < 0.7
            {
                let name = l.base.files[id].path.rsplit('/').next().unwrap();
                let label = if focused {
                    path_label(&l.base.files[id].path, 44)
                } else {
                    path_label(name, 26)
                };
                let size = if focused { 12. } else { 11. };
                let width = (a.text_width(&label, size, true) + 14.).min(vp.w - 12.);
                let candidates = [
                    Rect::new(r.x, r.y - 21., width, 20.),
                    Rect::new(r.x, r.y + r.h + 2., width, 20.),
                    Rect::new(r.x + r.w + 3., r.y, width, 20.),
                ];
                if let Some(lr) = candidates
                    .into_iter()
                    .map(|mut lr| {
                        lr.x = lr.x.clamp(vp.x + 6., vp.x + vp.w - lr.w - 6.);
                        lr.y = lr.y.clamp(vp.y + 2., vp.y + vp.h - lr.h - 2.);
                        lr
                    })
                    .find(|lr| {
                        !occupied.iter().any(|b| b.intersects(*lr))
                            && (self.selected == Some(id)
                                || !selected_screen.is_some_and(|b| b.intersects(*lr)))
                    })
                {
                    occupied.push(lr);
                    self.hits.push((lr, Command::File(id)));
                    rect(
                        &mut captions,
                        lr,
                        alpha(
                            rgb(if self.selected == Some(id) {
                                0x254e45
                            } else {
                                0x182a37
                            }),
                            opacity,
                        ),
                        3.,
                    );
                    if focused {
                        outline(&mut captions, lr, alpha(rgb(ACCENT), opacity), 1.);
                    }
                    a.middle_text(
                        &mut captions,
                        &label,
                        lr.x + 6.,
                        lr.y + 3.,
                        size,
                        alpha(rgb(TEXT), opacity),
                        lr.w - 12.,
                    );
                }
            }
        }
        out.append(&mut captions);
        rect(&mut out, Rect::new(0., 0., w, 72.), rgb(0x101c28), 0.);
        rect(
            &mut out,
            Rect::new(0., 72., SIDE, h - 72.),
            rgb(0x101923),
            0.,
        );
        outline(
            &mut out,
            Rect::new(0., 72., SIDE, h - 72.),
            rgb(0x223342),
            1.,
        );
        rect(&mut out, Rect::new(24., 24., 27., 27.), rgb(0x244b45), 6.);
        a.text(&mut out, "12", 28., 27., 15., rgb(ACCENT), true);
        a.text(&mut out, "CodeBush", 64., 21., 25., rgb(TEXT), true);
        let mode_x = 64. + a.text_width("CodeBush", 25., true) + 16.;
        self.button(
            &mut out,
            a,
            Rect::new(mode_x, 22., 100., 34.),
            "Tree view",
            Command::Mode,
            false,
        );
        a.text(
            &mut out,
            &ellipsis(&l.base.name, if w < 1500. { 17 } else { 28 }),
            SIDE + 24.,
            22.,
            18.,
            rgb(TEXT),
            true,
        );
        a.text(
            &mut out,
            "ORTHOGRAPHIC  /  12 DIMENSIONS",
            SIDE + 24.,
            46.,
            9.,
            rgb(MUTED),
            true,
        );
        let search = Rect::new((w - 650.).max(SIDE + 230.), 22., 240., 34.);
        rect(&mut out, search, rgb(0x0b141e), 5.);
        outline(
            &mut out,
            search,
            rgb(if self.editing { ACCENT } else { BORDER }),
            1.,
        );
        a.text(
            &mut out,
            if self.query.is_empty() {
                "Find connected file...  /"
            } else {
                &self.query
            },
            search.x + 10.,
            search.y + 9.,
            12.,
            rgb(MUTED),
            true,
        );
        self.hits.push((search, Command::Search));
        self.button(
            &mut out,
            a,
            Rect::new(w - 382., 22., 110., 34.),
            "Open folder",
            Command::Open,
            false,
        );
        self.button(
            &mut out,
            a,
            Rect::new(w - 260., 22., 137., 34.),
            if self.tests {
                "Tests included"
            } else {
                "Tests hidden"
            },
            Command::Tests,
            self.tests,
        );
        self.button(
            &mut out,
            a,
            Rect::new(w - 110., 22., 86., 34.),
            "Help  ?",
            Command::Help,
            self.help,
        );
        let [d1, d2] = PAIRS[self.plane];
        let label = if self.t < 1. {
            format!("Rotating to D{:02} × D{:02}", d1 + 1, d2 + 1)
        } else {
            format!("D{:02} × D{:02}", d1 + 1, d2 + 1)
        };
        rect(
            &mut out,
            Rect::new(SIDE, 72., w - SIDE, 54.),
            rgb(0x101923),
            0.,
        );
        a.text(&mut out, &label, SIDE + 24., 90., 18., rgb(TEXT), true);
        if let Some(id) = self.selected {
            a.text(
                &mut out,
                &path_label(
                    &l.base.files[id].path,
                    ((vp.w - 48.) / 6.).max(16.) as usize,
                ),
                SIDE + 24.,
                113.,
                9.,
                rgb(MUTED),
                true,
            );
        }
        let edge_count = graph
            .edges
            .iter()
            .filter(|e| (self.tests || !e.test_only) && self.weights[e.plane] > 1e-6)
            .count();
        a.text(
            &mut out,
            &format!(
                "{} files   ·   {} directed links",
                self.active.len(),
                edge_count
            ),
            SIDE + 360.,
            94.,
            12.,
            rgb(MUTED),
            true,
        );
        self.button(
            &mut out,
            a,
            Rect::new(w - 110., 82., 86., 33.),
            "Fit  F",
            Command::Fit,
            false,
        );
        if self.reading_origin.is_some() {
            self.button(
                &mut out,
                a,
                Rect::new(w - 286., 82., 164., 33.),
                "Back to links  Esc",
                Command::Back,
                false,
            );
        }
        a.text(
            &mut out,
            "COORDINATE PLANES",
            24.,
            96.,
            11.,
            rgb(MUTED),
            true,
        );
        a.text(
            &mut out,
            "Select a pair of dimensions",
            24.,
            118.,
            13.,
            rgb(TEXT),
            true,
        );
        let cell = 20.;
        let x0 = 49.;
        let y0 = 163.;
        for k in 0..12 {
            a.text(
                &mut out,
                &format!("{:02}", k + 1),
                x0 + k as f32 * cell + 3.,
                143.,
                9.,
                rgb(MUTED),
                false,
            );
            a.text(
                &mut out,
                &format!("{:02}", k + 1),
                24.,
                y0 + k as f32 * cell + 3.,
                9.,
                rgb(MUTED),
                false,
            );
        }
        let maximum = graph.plane_edges[self.tests as usize]
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        for (p, &[i, j]) in PAIRS.iter().enumerate() {
            let r = Rect::new(x0 + j as f32 * cell, y0 + i as f32 * cell, 17., 17.);
            let count = graph.plane_edges[self.tests as usize][p].len();
            let active = p == self.plane;
            rect(
                &mut out,
                r,
                if active {
                    rgb(0x58cdb1)
                } else if count == 0 {
                    rgb(0x18222e)
                } else {
                    alpha(rgb(0x458f84), 0.3 + 0.65 * (count as f32 / maximum).sqrt())
                },
                3.,
            );
            if active {
                a.text(&mut out, "•", r.x + 4., r.y, 12., rgb(0x0e302c), true);
            }
            self.hits.push((r, Command::Plane(p)));
        }
        a.text(
            &mut out,
            "66 planes · brighter cells contain more links",
            24.,
            413.,
            10.,
            rgb(MUTED),
            true,
        );
        a.text(
            &mut out,
            if self.candidates.is_some() {
                "OVERLAPPING SOURCE"
            } else if self.relations && self.selected.is_some() {
                "REFERENCE EVIDENCE"
            } else {
                "CONNECTED SOURCE"
            },
            24.,
            451.,
            11.,
            rgb(MUTED),
            true,
        );
        if self.selected.is_some() || self.candidates.is_some() {
            self.button(
                &mut out,
                a,
                Rect::new(212., 439., 75., 32.),
                if self.relations || self.candidates.is_some() {
                    "All files"
                } else {
                    "Links"
                },
                Command::Relations,
                self.relations,
            );
        }
        let q = self.query.to_lowercase();
        let filtered: Vec<_> = self
            .active
            .iter()
            .filter(|(id, _)| self.candidates.as_ref().is_none_or(|ids| ids.contains(id)))
            .filter(|(id, _)| q.is_empty() || l.base.files[*id].path.to_lowercase().contains(&q))
            .collect();
        let bottom = (h - 178.).max(505.);
        if let Some(id) = self.selected.filter(|_| self.relations) {
            let related: Vec<_> = graph
                .edges
                .iter()
                .filter(|e| {
                    (e.from == id || e.to == id)
                        && (self.tests || !e.test_only)
                        && self.weights[e.plane] > 1e-6
                })
                .collect();
            self.scroll = self
                .scroll
                .clamp(0., (related.len() as f32 * 66. - (bottom - 479.)).max(0.));
            for (row, e) in related.iter().enumerate() {
                let y = 479. + row as f32 * 66. - self.scroll;
                if y < 478. || y + 62. > bottom {
                    continue;
                }
                let other = if e.from == id { e.to } else { e.from };
                let r = Rect::new(14., y, SIDE - 28., 62.);
                rect(&mut out, r, rgb(0x182c34), 4.);
                a.text(
                    &mut out,
                    &path_label(&l.base.files[other].path, 36),
                    24.,
                    y + 7.,
                    11.,
                    rgb(TEXT),
                    true,
                );
                a.text(
                    &mut out,
                    &format!(
                        "{} · line {}",
                        if e.from == id {
                            "references"
                        } else {
                            "referenced by"
                        },
                        e.line
                    ),
                    24.,
                    y + 25.,
                    10.,
                    rgb(ACCENT),
                    true,
                );
                a.text(
                    &mut out,
                    &ellipsis(&e.spelling, 39),
                    24.,
                    y + 43.,
                    10.,
                    rgb(MUTED),
                    true,
                );
                self.hits.push((r, Command::File(other)));
            }
        } else {
            let maxscroll = (filtered.len() as f32 * 29. - (bottom - 479.)).max(0.);
            self.scroll = self.scroll.clamp(0., maxscroll);
            for (row, &&(id, _)) in filtered.iter().enumerate() {
                let y = 479. + row as f32 * 29. - self.scroll;
                if y < 478. || y + 27. > bottom {
                    continue;
                }
                let r = Rect::new(14., y, SIDE - 28., 27.);
                if self.selected == Some(id) {
                    rect(&mut out, r, rgb(0x203e39), 4.);
                }
                let label = path_label(&l.base.files[id].path, 37);
                a.text(
                    &mut out,
                    &label,
                    24.,
                    y + 6.,
                    11.,
                    rgb(if self.selected == Some(id) {
                        ACCENT
                    } else {
                        TEXT
                    }),
                    true,
                );
                self.hits.push((r, Command::File(id)));
            }
            if filtered.is_empty() {
                a.text(
                    &mut out,
                    "No connected files match.",
                    24.,
                    488.,
                    12.,
                    rgb(MUTED),
                    true,
                );
            }
        }
        rect(
            &mut out,
            Rect::new(0., h - 163., SIDE, 163.),
            rgb(0x111e2a),
            0.,
        );
        if let Some(id) = self.selected {
            a.text(
                &mut out,
                &path_label(&l.base.files[id].path, 36),
                24.,
                h - 145.,
                11.,
                rgb(TEXT),
                true,
            );
            let incoming = graph
                .edges
                .iter()
                .filter(|e| {
                    e.to == id && (self.tests || !e.test_only) && self.weights[e.plane] > 1e-6
                })
                .count();
            let outgoing = graph
                .edges
                .iter()
                .filter(|e| {
                    e.from == id && (self.tests || !e.test_only) && self.weights[e.plane] > 1e-6
                })
                .count();
            a.text(
                &mut out,
                &format!("{incoming} incoming · {outgoing} outgoing"),
                24.,
                h - 123.,
                11.,
                rgb(MUTED),
                true,
            );
            self.button(
                &mut out,
                a,
                Rect::new(24., h - 100., 262., 34.),
                "Read source  Enter",
                Command::Read,
                true,
            );
        } else {
            a.text(
                &mut out,
                "Follow the source.",
                24.,
                h - 142.,
                16.,
                rgb(TEXT),
                true,
            );
            a.text(
                &mut out,
                "Select a file to inspect its full code.",
                24.,
                h - 114.,
                11.,
                rgb(MUTED),
                true,
            );
            a.text(
                &mut out,
                "Arrowheads point to referenced files.",
                24.,
                h - 91.,
                10.,
                rgb(MUTED),
                true,
            );
        }
        if self.active.is_empty() {
            a.text(
                &mut out,
                "No direct links in this plane",
                vp.x + vp.w / 2. - 142.,
                vp.y + vp.h / 2. - 14.,
                20.,
                rgb(TEXT),
                true,
            );
            a.text(
                &mut out,
                "Choose a brighter cell in the plane matrix.",
                vp.x + vp.w / 2. - 132.,
                vp.y + vp.h / 2. + 20.,
                12.,
                rgb(MUTED),
                true,
            );
        }
        rect(
            &mut out,
            Rect::new(SIDE, h - 54., w - SIDE, 54.),
            rgb(0x111e2a),
            0.,
        );
        self.button(
            &mut out,
            a,
            Rect::new(SIDE + 18., h - 44., 82., 32.),
            "Previous",
            Command::Previous,
            false,
        );
        self.button(
            &mut out,
            a,
            Rect::new(SIDE + 108., h - 44., 65., 32.),
            "Next",
            Command::Next,
            false,
        );
        self.button(
            &mut out,
            a,
            Rect::new(SIDE + 181., h - 44., 106., 32.),
            if self.playing {
                "Pause tour"
            } else {
                "Tour planes"
            },
            Command::Play,
            self.playing,
        );
        let track = Rect::new(SIDE + 311., h - 32., (w - SIDE - 570.).max(75.), 5.);
        rect(&mut out, track, rgb(0x2e4252), 3.);
        rect(
            &mut out,
            Rect::new(track.x, track.y, track.w * self.t as f32, track.h),
            rgb(ACCENT),
            3.,
        );
        rect(
            &mut out,
            Rect::new(
                track.x + track.w * self.t as f32 - 4.,
                track.y - 4.,
                8.,
                13.,
            ),
            rgb(ACCENT),
            4.,
        );
        // Each hit segment is an explicit scrubbing position, not a camera interpolation.
        for i in 0..100 {
            self.hits.push((
                Rect::new(
                    track.x + track.w * i as f32 / 100.,
                    track.y - 12.,
                    track.w / 100. + 1.,
                    29.,
                ),
                Command::Scrub(i as f64 / 99.),
            ));
        }
        a.text(
            &mut out,
            &if self.fps > 0. {
                format!(
                    "{:.0} fps · {:.2} GB resident",
                    self.fps,
                    bytes as f64 / 1e9
                )
            } else {
                format!("GPU resident · {:.2} GB", bytes as f64 / 1e9)
            },
            w - 226.,
            h - 31.,
            11.,
            rgb(MUTED),
            true,
        );
        a.text(
            &mut out,
            &format!(
                "{} links · {} unresolved",
                graph.edges.len(),
                graph.unresolved.len()
            ),
            24.,
            h - 39.,
            10.,
            rgb(MUTED),
            true,
        );
        if !self.status.is_empty() {
            rect(
                &mut out,
                Rect::new(SIDE + 18., 137., vp.w - 36., 35.),
                rgb(0x273b46),
                5.,
            );
            a.text(
                &mut out,
                &ellipsis(&self.status, 100),
                SIDE + 30.,
                148.,
                12.,
                rgb(TEXT),
                true,
            );
        }
        // Full file identity is always available on hover, including long siblings.
        let row_hover = self.hits.iter().rev().find_map(|(r, c)| {
            if r.contains(self.pointer) {
                if let Command::File(id) = c {
                    Some(*id)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let hover = row_hover
            .or_else(|| self.selected.filter(|id| self.hover_ids.contains(id)))
            .or_else(|| self.hover_ids.first().copied());
        if let Some(id) = hover.filter(|&id| {
            !self.hover_suppressed
                && self.hover_age > 0.35
                && (row_hover.is_some()
                    || self.hover_ids.len() > 1
                    || self.camera.zoom * hg.reading_scale(self.tests as usize, id) < 0.7)
        }) {
            let width = (vp.w - 30.).clamp(240., 800.);
            let chars: Vec<_> = l.base.files[id].path.chars().collect();
            let columns = ((width - 24.) / 6.6) as usize;
            let lines: Vec<String> = chars
                .chunks(columns.max(1))
                .map(|s| s.iter().collect())
                .collect();
            let overlap = row_hover.is_none() && self.hover_ids.len() > 1;
            let height = 18. + lines.len() as f32 * 17. + if overlap { 23. } else { 0. };
            let x =
                (self.pointer[0] + 16.).clamp(vp.x + 8., (vp.x + vp.w - width - 8.).max(vp.x + 8.));
            let y = (self.pointer[1] + 20.)
                .clamp(vp.y + 8., (vp.y + vp.h - height - 8.).max(vp.y + 8.));
            rect(&mut out, Rect::new(x, y, width, height), rgb(0x243942), 5.);
            outline(&mut out, Rect::new(x, y, width, height), rgb(0x548878), 1.);
            for (line, s) in lines.iter().enumerate() {
                a.text(
                    &mut out,
                    s,
                    x + 12.,
                    y + 9. + line as f32 * 17.,
                    11.,
                    rgb(TEXT),
                    true,
                );
            }
            if overlap {
                a.text(
                    &mut out,
                    &format!(
                        "{} overlapping files · O lists every candidate · Shift-click cycles",
                        self.hover_ids.len()
                    ),
                    x + 12.,
                    y + height - 24.,
                    10.,
                    rgb(ACCENT),
                    true,
                );
            }
        }
        if self.help {
            rect(
                &mut out,
                Rect::new(0., 0., w, h),
                alpha(rgb(0x050a10), 0.88),
                0.,
            );
            let r = Rect::new(w / 2. - 300., h / 2. - 250., 600., 500.);
            rect(&mut out, r, rgb(0x152532), 10.);
            outline(&mut out, r, rgb(0x3f665e), 1.);
            a.text(
                &mut out,
                "Explore twelve dimensions",
                r.x + 30.,
                r.y + 28.,
                24.,
                rgb(TEXT),
                true,
            );
            for (i, line) in [
                "Choose any of the 66 pairs in the plane matrix.",
                "Scroll to zoom · drag to pan · F to fit the connected graph.",
                "Click to trace a file's links; Enter reads. Shift-click cycles overlaps.",
                "Hover for full paths. O lists the source panels under the pointer.",
                "[ / ] selects the previous / next plane. Space tours all planes.",
                "The bottom slider scrubs the current rotation. Escape closes help.",
                "Rotations preserve 12D distances; views are orthographic.",
                "Source panels are readable annotations at fixed file anchors.",
                "Crossing projected links do not create graph connections.",
                "Each link belongs to one stable, path-hashed plane layer.",
                "Layers filter the graph; they do not describe edge directions.",
                "Links are static imports/modules/includes; unresolved references",
                "and parser coverage are recorded in the dependency report.",
                "Tree view switches modes instantly. Ctrl O opens a folder; Ctrl R reloads.",
            ]
            .iter()
            .enumerate()
            {
                a.text(
                    &mut out,
                    line,
                    r.x + 30.,
                    r.y + 82. + i as f32 * 25.,
                    13.,
                    rgb(if i >= 5 { MUTED } else { TEXT }),
                    true,
                );
            }
        }
        out
    }
}
fn ellipsis(s: &str, max: usize) -> String {
    let mut c = s.chars();
    let mut out: String = c.by_ref().take(max).collect();
    if c.next().is_some() {
        out.push('…');
    }
    out
}
fn path_label(s: &str, max: usize) -> String {
    let c: Vec<_> = s.chars().collect();
    if c.len() <= max {
        return s.to_owned();
    }
    let tail = s
        .rsplit('/')
        .next()
        .unwrap_or(s)
        .chars()
        .count()
        .max(max * 2 / 3)
        .min(max.saturating_sub(5));
    format!(
        "{}…{}",
        c[..max - tail - 1].iter().collect::<String>(),
        c[c.len() - tail..].iter().collect::<String>()
    )
}
fn resident(
    args: &Args,
    l: &Loaded,
    gpu: &mut Gpu,
    graph: &Graph,
) -> Result<([GpuScene; 2], HyperGpu, u64)> {
    let baseline = budget(args, l, gpu)?;
    let mut residents = [gpu.upload(&l.scenes[0]), gpu.upload(&l.scenes[1])];
    let remaining = (args.vram_gb * 1e9) as u64 - baseline;
    for i in 0..2 {
        gpu.cache_scene(&l.scenes[i], &mut residents[i], remaining / 2)?;
    }
    let hyper = HyperGpu::new(gpu, graph, &l.scenes, &residents);
    let bytes = residents.iter().map(|r| r.bytes).sum::<u64>() + gpu.atlas_bytes + hyper.bytes;
    anyhow::ensure!(
        bytes + 512 * 1024 * 1024 <= (args.vram_gb * 1e9) as u64,
        "12D scene exceeds configured GPU budget"
    );
    gpu.wait(None)?;
    Ok((residents, hyper, bytes))
}
fn report(path: &Path, l: &Loaded, graph: &Graph, gpu: &Gpu, bytes: u64, ready: f64) -> Result<()> {
    std::fs::create_dir_all(path)?;
    std::fs::write(
        path.join("dependencies.json"),
        serde_json::to_vec(
            &serde_json::json!({"root":l.base.root,"files":l.base.files.iter().map(|f|&f.path).collect::<Vec<_>>(),"graph":graph,"semantics":"Static imports/modules/includes and resolvable qualified Rust paths. Unresolved references and parser coverage are explicit. Edge planes are stable reference layers, independent of geometric direction."}),
        )?,
    )?;
    std::fs::write(
        path.join("load-report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"root":l.base.root,"stats":l.base.stats,"gpu":gpu.info.name,"resident_bytes":bytes,"ready_ms":ready,"links":graph.edges.len(),"unresolved":graph.unresolved.len(),"parse_failures":graph.parse_failures.len(),"tree_layout_tests_off":l.scenes[0].layout_stats(),"tree_layout_tests_on":l.scenes[1].layout_stats(),"validation_errors":gpu.errors.lock().unwrap().clone()}),
        )?,
    )?;
    Ok(())
}
pub fn run(args: Args, l: Loaded) -> Result<()> {
    let started = Instant::now();
    eprintln!("Resolving direct source references in 12D");
    let graph = Graph::build(&l.base);
    eprintln!(
        "{} directed links, {} unresolved, {} parse failures",
        graph.edges.len(),
        graph.unresolved.len(),
        graph.parse_failures.len()
    );
    if args.capture.is_some() || args.benchmark {
        return headless(&args, l, graph, started);
    }
    let event_loop = EventLoop::new()?;
    let mut app = HyperApp {
        args,
        load: Some((l, graph, started)),
        running: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(e) = app.error {
        anyhow::bail!(e);
    }
    Ok(())
}
fn headless(args: &Args, l: Loaded, graph: Graph, started: Instant) -> Result<()> {
    let size = parse_size(&args.size)?;
    let dpi = size[0] as f32 / 1920.;
    let logical = [size[0] as f32 / dpi, size[1] as f32 / dpi];
    let (mut gpu, _) = pollster::block_on(Gpu::new(&l.atlas, None, args.allow_other_gpu))?;
    let (residents, hg, bytes) = resident(args, &l, &mut gpu, &graph)?;
    let mut state = View::new(&graph, !args.no_tests);
    state.refresh(&graph);
    state.fit(&hg, logical);
    state.camera = state.target;
    let out = args
        .capture
        .clone()
        .unwrap_or_else(|| PathBuf::from("evidence/12d"));
    let ready = l.load_ms + started.elapsed().as_secs_f64() * 1000.;
    report(&out, &l, &graph, &gpu, bytes, ready)?;
    let target = gpu.target(size[0], size[1]);
    let view = target.create_view(&Default::default());
    let mut capture = |name: &str, state: &mut View| -> Result<()> {
        state.refresh(&graph);
        let ui = state.draw(&l, &graph, &hg, logical, bytes);
        let i = state.tests as usize;
        let sub = hg.render(
            &mut gpu,
            &view,
            size,
            dpi,
            viewport(logical),
            state.camera,
            &state.matrix,
            i,
            &l.scenes[i],
            &residents[i],
            &state.active,
            state.selected,
            &ui,
            false,
        );
        gpu.wait(Some(sub.submission))?;
        gpu.save_png(&target, &out.join(format!("{name}.png")))?;
        Ok(())
    };
    if args.capture.is_some() {
        capture("01-connected-plane", &mut state)?;
        let mut degrees = vec![0usize; l.base.files.len()];
        for e in &graph.edges {
            if (state.tests || !e.test_only) && state.weights[e.plane] > 1e-6 {
                degrees[e.from] += 1;
                degrees[e.to] += 1;
            }
        }
        if let Some(&(id, _)) = state.active.iter().max_by_key(|(id, _)| degrees[*id]) {
            state.select_file(id, &hg, logical);
            capture("13-selected-neighborhood", &mut state)?;
            if let Some(other) = graph.edges.iter().find_map(|e| {
                ((state.tests || !e.test_only) && state.weights[e.plane] > 1e-6)
                    .then(|| {
                        if e.from == id {
                            Some(e.to)
                        } else if e.to == id {
                            Some(e.from)
                        } else {
                            None
                        }
                    })
                    .flatten()
            }) {
                state.select_file(other, &hg, logical);
                capture("14-followed-reference", &mut state)?;
                let before = state.camera;
                state.focus(other, &hg, logical, true);
                state.camera = state.target;
                let reading_camera = state.camera;
                state.select_file(other, &hg, logical);
                anyhow::ensure!(
                    state.target.center == reading_camera.center
                        && state.target.zoom == reading_camera.zoom,
                    "Selecting the current reading card must retain its camera"
                );
                capture("15-reference-source", &mut state)?;
                state.back_to_links();
                state.camera = state.target;
                anyhow::ensure!(
                    state.camera.center == before.center && state.camera.zoom == before.zoom,
                    "Reading a reference did not preserve the previous graph framing"
                );
                capture("16-return-to-links", &mut state)?;
            }
            state.selected = None;
            state.relations = false;
        }
        let first = state.plane;
        let [a, b] = PAIRS[first];
        let next = (0..PLANES)
            .filter(|&p| !PAIRS[p].contains(&a) && !PAIRS[p].contains(&b))
            .max_by_key(|&p| graph.plane_edges[1][p].len())
            .unwrap();
        state.turn(next, &graph, &hg, logical);
        state.camera = state.target;
        state.t = 0.25;
        capture("02-rotation-quarter", &mut state)?;
        state.t = 0.5;
        capture("03-rotation-half", &mut state)?;
        let overlap = state.active.iter().take(400).find_map(|&(id, _)| {
            let b = hg.bounds(1, id, &state.matrix);
            state
                .active
                .iter()
                .take(400)
                .find(|&&(other, _)| {
                    other != id && hg.bounds(1, other, &state.matrix).intersects(b)
                })
                .map(|&(other, _)| (id, other))
        });
        if let Some((id, other)) = overlap {
            state.focus(id, &hg, logical, false);
            state.camera = state.target;
            state.selected = Some(other);
            capture("09-overlap-before", &mut state)?;
            state.selected = Some(id);
            capture("10-overlap-raised", &mut state)?;
            state.focus(id, &hg, logical, true);
            state.camera = state.target;
            state.relations = true;
            capture("11-reference-evidence", &mut state)?;
            state.relations = false;
        }
        state.t = 1.;
        state.refresh(&graph);
        state.fit(&hg, logical);
        state.camera = state.target;
        capture("04-other-plane", &mut state)?;
        if let Some(&(id, _)) = state
            .active
            .iter()
            .find(|&&(id, _)| l.base.files[id].lines.len() > 20)
            .or(state.active.first())
        {
            state.focus(id, &hg, logical, true);
            state.camera = state.target;
            capture("05-source-reading", &mut state)?;
        }
        state.tests = false;
        state.refresh(&graph);
        state.fit(&hg, logical);
        state.camera = state.target;
        capture("06-tests-hidden", &mut state)?;
        let empty = (0..PLANES)
            .min_by_key(|&p| graph.plane_edges[0][p].len())
            .unwrap();
        state.turn(empty, &graph, &hg, logical);
        state.t = 1.;
        state.refresh(&graph);
        state.fit(&hg, logical);
        state.camera = state.target;
        capture("07-sparse-plane", &mut state)?;
        state.help = true;
        capture("08-controls", &mut state)?;
        state.help = false;
        let mut siblings: std::collections::BTreeMap<
            (usize, String),
            std::collections::BTreeSet<usize>,
        > = std::collections::BTreeMap::new();
        for e in &graph.edges {
            for id in [e.from, e.to] {
                if let Some((parent, _)) = l.base.files[id].path.rsplit_once('/') {
                    if parent.len() >= 24 {
                        siblings
                            .entry((e.plane, parent.to_owned()))
                            .or_default()
                            .insert(id);
                    }
                }
            }
        }
        if let Some(((plane, prefix), _)) = siblings
            .iter()
            .filter(|(_, ids)| ids.len() > 1)
            .max_by_key(|(_, ids)| ids.len())
        {
            state.tests = true;
            state.selected = None;
            state.turn(*plane, &graph, &hg, logical);
            state.t = 1.;
            state.refresh(&graph);
            state.fit(&hg, logical);
            state.camera = state.target;
            state.query = prefix.clone();
            state.pointer = [50., 490.];
            state.hover_age = 1.;
            capture("12-filenames-and-full-path", &mut state)?;
            state.query.clear();
            state.pointer = [-1., -1.];
        }
    }
    if args.benchmark {
        let warmup = 120;
        let total = args.frames + warmup;
        gpu.configure_timing(total as u32)?;
        let mut samples = Vec::new();
        let (tx, rx) = mpsc::channel();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let start = Instant::now();
        let mut measurement = start;
        let hz = if args.benchmark_hz > 0. {
            args.benchmark_hz
        } else {
            120.
        };
        let dt = Duration::from_secs_f64(1. / hz);
        let mut due = start;
        let mut max_error = 0f64;
        for f in 0..total {
            while f - done.load(std::sync::atomic::Ordering::Acquire)
                >= args.pipeline_depth as usize
                || Instant::now() < due
            {
                gpu.device.poll(wgpu::PollType::Poll)?;
                std::hint::spin_loop();
            }
            let now = Instant::now();
            if f == warmup {
                measurement = now;
            }
            let late = now.saturating_duration_since(due).as_secs_f64() * 1000.;
            due += dt;
            let block = (f + PLANES * 60 - warmup) / 60;
            let p = (block * 17) % PLANES;
            state.tests = (block / PLANES) % 2 == 1;
            let profile = (block + block / PLANES) % 3;
            if f % 60 == 0 {
                state.turn(p, &graph, &hg, logical);
                state.camera = state.target;
            }
            state.t = (f % 60) as f64 / 59.;
            state.refresh(&graph);
            if profile != 0 {
                if let Some(&(id, _)) = state
                    .active
                    .iter()
                    .find(|(id, _)| l.base.files[*id].lines.len() > 20)
                    .or(state.active.first())
                {
                    state.focus(id, &hg, logical, true);
                    if profile == 2 {
                        // Cross the real source-image/glyph boundary throughout rotation.
                        state.target.zoom = hg.cache_scale(state.tests as usize, id)
                            / (dpi * hg.scale(state.tests as usize, id))
                            * (0.8 + 0.4 * state.t as f32);
                        state.target.center =
                            hg.bounds(state.tests as usize, id, &state.matrix).center();
                    }
                    state.camera = state.target;
                }
            }
            max_error = max_error.max(projection::orthogonality_error(&state.matrix));
            let projection_ms = now.elapsed().as_secs_f64() * 1000.;
            let ui_start = Instant::now();
            let ui = state.draw(&l, &graph, &hg, logical, bytes);
            let ui_ms = ui_start.elapsed().as_secs_f64() * 1000.;
            gpu.query_slot = f as u32;
            let mode = state.tests as usize;
            let encode_start = Instant::now();
            let drawn = hg.render(
                &mut gpu,
                &view,
                size,
                dpi,
                viewport(logical),
                state.camera,
                &state.matrix,
                mode,
                &l.scenes[mode],
                &residents[mode],
                &state.active,
                state.selected,
                &ui,
                true,
            );
            let encode_ms = encode_start.elapsed().as_secs_f64() * 1000.;
            let tx = tx.clone();
            let done = done.clone();
            gpu.queue.on_submitted_work_done(move || {
                let _ = tx.send((f, now.elapsed().as_secs_f64() * 1000.));
                done.fetch_add(1, std::sync::atomic::Ordering::Release);
            });
            if f >= warmup {
                samples.push(serde_json::json!({"plane":p,"t":state.t,"tests":state.tests,"rotation_factors":state.rotation.factors.len(),"projection_ms":projection_ms,"encode_ms":encode_ms,"profile":(["overview","source-reading","cache-transition"][profile]),"detail_instances":drawn.detail_instances,"active_files":state.active.len(),"ui_ms":ui_ms,"start_lateness_ms":late}));
            }
        }
        while done.load(std::sync::atomic::Ordering::Acquire) < total {
            gpu.device.poll(wgpu::PollType::Poll)?;
            std::hint::spin_loop();
        }
        let cadence = args.frames as f64 / measurement.elapsed().as_secs_f64();
        for (f, ms) in rx.try_iter() {
            if f >= warmup {
                let s = &mut samples[f - warmup];
                s["completion_ms"] = ms.into();
                s["deadline_ms"] = (ms + s["start_lateness_ms"].as_f64().unwrap()).into();
            }
        }
        let times = gpu.gpu_frame_times()?;
        for (s, t) in samples.iter_mut().zip(times.into_iter().skip(warmup)) {
            s["gpu_ms"] = t.into();
        }
        let pct = |key: &str| {
            let mut a: Vec<_> = samples.iter().map(|s| s[key].as_f64().unwrap()).collect();
            a.sort_by(f64::total_cmp);
            a[((a.len() - 1) as f64 * 0.99) as usize]
        };
        let errors = gpu.errors.lock().unwrap().clone();
        let coverage = (0..2)
            .map(|mode| {
                (0..PLANES)
                    .filter(|&plane| {
                        samples.iter().any(|s| {
                            s["plane"] == plane
                                && s["tests"] == (mode == 1)
                                && s["t"].as_f64().unwrap() > 0.
                                && s["t"].as_f64().unwrap() < 1.
                                && s["rotation_factors"].as_u64().unwrap() > 0
                        })
                    })
                    .count()
            })
            .collect::<Vec<_>>();
        if args.frames >= 7920 {
            anyhow::ensure!(
                coverage == [66, 66],
                "Incomplete rotation coverage: {coverage:?}"
            );
        }

        let report = serde_json::json!({"adapter":gpu.info.name,"resolution":size,"source_files":l.base.stats.files,"source_lines":l.base.stats.lines,"links":graph.edges.len(),"resident_bytes":bytes,"ready_ms":ready,"frames":args.frames,"rotating_planes_tests_off_on":coverage,"projection_p99_ms":pct("projection_ms"),"encode_p99_ms":pct("encode_ms"),"cadence_fps":cadence,"gpu_p99_ms":pct("gpu_ms"),"ui_p99_ms":pct("ui_ms"),"completion_p99_ms":pct("completion_ms"),"deadline_p99_ms":pct("deadline_ms"),"passes_120fps_p99":pct("deadline_ms")<=1000./120.&&errors.is_empty(),"orthogonality_max_error":max_error,"rendering_errors":errors,"frames_detail":samples,"measurement":"True 8K offscreen GPU completion, not physical monitor presentation. Every coordinate plane is reached with tests on and off when frames >=7920."});
        std::fs::write(
            out.join("benchmark.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        eprintln!(
            "12D GPU p99 {:.3}ms, completion p99 {:.3}ms, cadence {:.2}",
            pct("gpu_ms"),
            pct("completion_ms"),
            cadence
        );
    }
    anyhow::ensure!(
        gpu.errors.lock().unwrap().is_empty(),
        "GPU validation errors recorded"
    );
    Ok(())
}
struct Uploaded {
    l: Loaded,
    graph: Graph,
    gpu: Gpu,
    surface: wgpu::Surface<'static>,
    residents: [GpuScene; 2],
    hg: HyperGpu,
    bytes: u64,
}
struct Live {
    l: Loaded,
    graph: Graph,
    gpu: Gpu,
    residents: [GpuScene; 2],
    hg: Option<HyperGpu>,
    bytes: u64,
    state: View,
    tree: crate::tree_view::TreeView,
    tree_mode: bool,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: [f32; 2],
    dpi: f32,
    cursor: [f32; 2],
    drag: bool,
    scrubbing: bool,
    moved: f32,
    last_click: Instant,
    last: Instant,
    next: Instant,
    stats: Instant,
    frames: usize,
    total: usize,
    commands: u64,
    modifiers: ModifiersState,
    timings: Vec<serde_json::Value>,
    folder: Option<mpsc::Receiver<Option<PathBuf>>>,
    reload: Option<mpsc::Receiver<Result<(Loaded, Graph)>>>,
    upload: Option<mpsc::Receiver<Result<Uploaded>>>,
    upload_started: Instant,
    loading_name: String,
}
impl Live {
    fn new(
        args: &Args,
        l: Loaded,
        graph: Graph,
        started: Instant,
        el: &ActiveEventLoop,
    ) -> Result<Self> {
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title(format!("CodeBush 12D — {}", l.base.name))
                    .with_inner_size(winit::dpi::LogicalSize::new(1760., 1080.))
                    .with_min_inner_size(winit::dpi::LogicalSize::new(1200., 850.)),
            )?,
        );
        let (mut gpu, surface) = pollster::block_on(Gpu::new(
            &l.atlas,
            Some(window.clone()),
            args.allow_other_gpu,
        ))?;
        let (residents, hg, bytes) = resident(args, &l, &mut gpu, &graph)?;
        let surface = surface.unwrap();
        let ps = window.inner_size();
        let dpi = window.scale_factor() as f32;
        let size = [ps.width as f32 / dpi, ps.height as f32 / dpi];
        let caps = surface.get_capabilities(&gpu.adapter);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.format,
            width: ps.width,
            height: ps.height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        let mut state = View::new(&graph, !args.no_tests);
        state.refresh(&graph);
        state.fit(&hg, size);
        state.camera = state.target;
        let path = native_report_path().parent().unwrap().join("12d-latest");
        report(
            &path,
            &l,
            &graph,
            &gpu,
            bytes,
            l.load_ms + started.elapsed().as_secs_f64() * 1000.,
        )?;
        let tree = crate::tree_view::TreeView::new(&l, window.clone(), size, dpi, !args.no_tests);
        window.set_title(&format!(
            "CodeBush {} — {}",
            if args.tree { "Tree" } else { "12D" },
            l.base.name
        ));
        Ok(Self {
            l,
            graph,
            gpu,
            residents,
            hg: Some(hg),
            bytes,
            state,
            tree,
            tree_mode: args.tree,
            window,
            surface,
            config,
            size,
            dpi,
            cursor: [0.; 2],
            drag: false,
            scrubbing: false,
            moved: 0.,
            last_click: Instant::now() - Duration::from_secs(1),
            last: Instant::now(),
            next: Instant::now(),
            stats: Instant::now(),
            frames: 0,
            total: 0,
            commands: 0,
            modifiers: ModifiersState::default(),
            timings: vec![],
            folder: None,
            reload: None,
            upload: None,
            upload_started: Instant::now(),
            loading_name: String::new(),
        })
    }
    fn command(&mut self, c: Command) {
        if self.hg.is_none() {
            return;
        }
        self.commands += 1;
        match c {
            Command::Mode => {
                self.state.playing = false;
                self.state.animating = false;
                self.state.framing = None;
                self.drag = false;
                self.scrubbing = false;
                self.tree.dragging = false;
                if !self.tree_mode {
                    self.tree.set_tests(&self.l, self.state.tests);
                    self.tree.cursor = self.cursor;
                }
                self.state.pointer = self.cursor;
                self.tree_mode = !self.tree_mode;
                self.window.set_title(&format!(
                    "CodeBush {} — {}",
                    if self.tree_mode { "Tree" } else { "12D" },
                    self.l.base.name
                ));
            }
            Command::Plane(p) => {
                self.state.playing = false;
                self.state
                    .turn(p, &self.graph, &self.hg.as_ref().unwrap(), self.size);
            }
            Command::Previous | Command::Next => {
                let p = if matches!(c, Command::Next) {
                    (self.state.plane + 1) % PLANES
                } else {
                    (self.state.plane + PLANES - 1) % PLANES
                };
                self.state
                    .turn(p, &self.graph, &self.hg.as_ref().unwrap(), self.size);
            }
            Command::Tests => {
                self.state.tests = !self.state.tests;
                self.state.refresh(&self.graph);
                self.state.fit(&self.hg.as_ref().unwrap(), self.size);
                self.state.camera = self.state.target;
                self.state.framing = None;
            }
            Command::Fit => self.state.fit(&self.hg.as_ref().unwrap(), self.size),
            Command::File(id) => {
                self.state
                    .select_file(id, self.hg.as_ref().unwrap(), self.size);
            }
            Command::Read => {
                if let Some(id) = self.state.selected {
                    self.state
                        .focus(id, &self.hg.as_ref().unwrap(), self.size, true);
                }
            }
            Command::Back => self.state.back_to_links(),
            Command::Play => {
                self.state.playing = !self.state.playing;
                self.state.animating = self.state.playing;
                if self.state.playing && self.state.t < 1. {
                    self.state.fit(self.hg.as_ref().unwrap(), self.size);
                    self.state.framing = Some((self.state.camera, 0.));
                }
                if self.state.playing && self.state.t >= 1. {
                    self.command(Command::Next);
                }
            }
            Command::Scrub(t) => {
                self.state.reading_origin = None;
                self.state.playing = false;
                self.state.t = t;
                self.state.animating = false;
                self.state.refresh(&self.graph);
                if let Some(bounds) = self.state.rotation_bounds {
                    self.state.target.fit(bounds, viewport(self.size));
                    self.state.camera = self.state.target;
                    self.state.framing = None;
                    self.state.fitted = true;
                } else {
                    self.state.fit(self.hg.as_ref().unwrap(), self.size);
                    self.state.camera = self.state.target;
                }
            }
            Command::Search => self.state.editing = true,
            Command::Help => self.state.help = !self.state.help,
            Command::Relations => {
                self.state.relations = if self.state.candidates.take().is_some() {
                    false
                } else {
                    !self.state.relations
                };
                self.state.scroll = 0.;
            }
            Command::Overlaps => {
                if self.state.hover_ids.len() > 1 {
                    self.state.candidates = Some(self.state.hover_ids.clone());
                    self.state.relations = false;
                    self.state.scroll = 0.;
                    self.state.query.clear();
                    self.state.playing = false;
                    self.state.animating = false;
                }
            }
            Command::Reload => self.load(self.l.base.root.clone()),
            Command::Open => {
                #[cfg(windows)]
                {
                    let (tx, rx) = mpsc::channel();
                    self.folder = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(
                            rfd::FileDialog::new()
                                .set_title("Open source code folder")
                                .pick_folder(),
                        );
                    });
                }
            }
        }
    }
    fn scrub_at(&mut self, x: f32) {
        let width = (self.size[0] - SIDE - 570.).max(75.);
        self.command(Command::Scrub(
            ((x - SIDE - 311.) / width).clamp(0., 1.) as f64
        ));
    }
    fn load(&mut self, path: PathBuf) {
        if self.reload.is_some() || self.upload.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.reload = Some(rx);
        self.state.status = "Preparing source and reference graph...".into();
        self.tree.state.loading = true;
        std::thread::spawn(move || {
            let _ = tx.send(prepare(&path).map(|mut l| {
                let started = Instant::now();
                let g = Graph::build(&l.base);
                l.load_ms += started.elapsed().as_secs_f64() * 1000.;
                (l, g)
            }));
        });
    }
    fn tick(&mut self, args: &Args) -> Result<()> {
        if let Some(rx) = &self.folder {
            if let Ok(path) = rx.try_recv() {
                self.folder = None;
                if let Some(path) = path {
                    self.load(path);
                }
            }
        }
        if let Some(rx) = &self.reload {
            if let Ok(result) = rx.try_recv() {
                self.reload = None;
                match result {
                    Err(e) => {
                        self.state.status = format!("Load failed: {e:#}");
                        self.tree.state.status = self.state.status.clone();
                        self.tree.state.loading = false;
                    }
                    Ok((l, graph)) => {
                        // Retire large source allocations on the worker. The old device
                        // remains alive for the responsive loading interface only.
                        budget(args, &l, &self.gpu)?;
                        self.upload_started = Instant::now();
                        self.loading_name = l.base.name.clone();
                        let retired = (
                            self.hg.take(),
                            std::mem::replace(
                                &mut self.residents,
                                [GpuScene::empty(), GpuScene::empty()],
                            ),
                        );
                        let old_device = self.gpu.device.clone();
                        let instance = self.gpu.instance.clone();
                        let surface = instance.create_surface(self.window.clone())?;
                        let args = args.clone();
                        self.state.hits.clear();
                        self.state.hover_ids.clear();
                        self.state.playing = false;
                        self.state.animating = false;
                        self.drag = false;
                        self.scrubbing = false;
                        let (tx, rx) = mpsc::channel();
                        self.upload = Some(rx);
                        std::thread::spawn(move || {
                            let result = (|| -> Result<Uploaded> {
                                let started = Instant::now();
                                old_device.poll(wgpu::PollType::wait_indefinitely())?;
                                drop(retired);
                                old_device.poll(wgpu::PollType::Poll)?;
                                let (mut gpu, surface) = pollster::block_on(Gpu::with_surface(
                                    &l.atlas,
                                    instance,
                                    Some(surface),
                                    args.allow_other_gpu,
                                ))?;
                                let (residents, hg, bytes) = resident(&args, &l, &mut gpu, &graph)?;
                                report(
                                    &native_report_path().parent().unwrap().join("12d-latest"),
                                    &l,
                                    &graph,
                                    &gpu,
                                    bytes,
                                    l.load_ms + started.elapsed().as_secs_f64() * 1000.,
                                )?;
                                Ok(Uploaded {
                                    l,
                                    graph,
                                    gpu,
                                    surface: surface.unwrap(),
                                    residents,
                                    hg,
                                    bytes,
                                })
                            })();
                            let _ = tx.send(result);
                        });
                    }
                }
            }
        }
        if let Some(rx) = &self.upload {
            if let Ok(result) = rx.try_recv() {
                self.upload = None;
                let next = result?;
                self.surface = next.surface;
                self.gpu = next.gpu;
                self.hg = Some(next.hg);
                self.residents = next.residents;
                self.bytes = next.bytes;
                let retired = (
                    std::mem::replace(&mut self.l, next.l),
                    std::mem::replace(&mut self.graph, next.graph),
                );
                // Releasing millions of CPU glyphs must not pause the window either.
                std::thread::spawn(move || drop(retired));
                let mut tree = crate::tree_view::TreeView::new(
                    &self.l,
                    self.window.clone(),
                    self.size,
                    self.dpi,
                    self.state.tests,
                );
                tree.cursor = self.cursor;
                let old_tree = std::mem::replace(&mut self.tree, tree);
                std::thread::spawn(move || drop(old_tree));
                self.state = View::new(&self.graph, self.state.tests);
                self.state.pointer = self.cursor;
                self.state.refresh(&self.graph);
                self.state.fit(self.hg.as_ref().unwrap(), self.size);
                self.state.camera = self.state.target;
                self.config.format = self.gpu.format;
                self.surface.configure(&self.gpu.device, &self.config);
                self.window.set_title(&format!(
                    "CodeBush {} — {}",
                    if self.tree_mode { "Tree" } else { "12D" },
                    self.l.base.name
                ));
            }
        }
        let start = Instant::now();
        let dt = start.duration_since(self.last).as_secs_f64();
        self.last = start;
        self.state.hover_age += dt as f32;
        let late = start.saturating_duration_since(self.next).as_secs_f64() * 1000.;
        while self.next <= start {
            self.next += Duration::from_nanos(8_333_333);
        }
        if let Some((from, elapsed)) = self.state.framing {
            let elapsed = (elapsed + dt / 0.28).min(1.);
            let blend =
                (elapsed * elapsed * elapsed * (10. + elapsed * (-15. + 6. * elapsed))) as f32;
            // Interpolate visible world extents, keeping zoom-out gentle from reading scale.
            self.state.camera.zoom =
                1. / (1. / from.zoom + (1. / self.state.target.zoom - 1. / from.zoom) * blend);
            for k in 0..2 {
                self.state.camera.center[k] =
                    from.center[k] + (self.state.target.center[k] - from.center[k]) * blend;
            }
            self.state.framing = if elapsed < 1. {
                Some((from, elapsed))
            } else {
                None
            };
        } else if !self.tree_mode && self.state.animating && self.state.t < 1. {
            self.state.t = (self.state.t + dt / 1.2).min(1.);
            self.state.refresh(&self.graph);
        } else if !self.tree_mode && self.state.playing {
            self.command(Command::Next);
        }
        if self.state.framing.is_none() {
            let blend = 1. - (-24. * dt.min(0.1) as f32).exp();
            self.state.camera.zoom += (self.state.target.zoom - self.state.camera.zoom) * blend;
            for k in 0..2 {
                self.state.camera.center[k] +=
                    (self.state.target.center[k] - self.state.camera.center[k]) * blend;
            }
        }
        let ui_start = Instant::now();
        let ui = if self.tree_mode && self.hg.is_some() {
            self.tree.draw(
                &self.l,
                dt,
                self.bytes,
                self.state.fps,
                &format!("{:?}", self.gpu.info.backend),
            )
        } else if let Some(hg) = &self.hg {
            self.state
                .draw(&self.l, &self.graph, hg, self.size, self.bytes)
        } else {
            self.loading_ui()
        };
        let ui_ms = ui_start.elapsed().as_secs_f64() * 1000.;
        let acquire = Instant::now();
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.gpu.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => anyhow::bail!("Surface validation error"),
        };
        let acquire_ms = acquire.elapsed().as_secs_f64() * 1000.;
        let view = frame.texture.create_view(&Default::default());
        let mode = self.state.tests as usize;
        if self.tree_mode && self.hg.is_some() {
            self.gpu.render(
                &view,
                [self.config.width, self.config.height],
                self.dpi,
                ui::viewport(self.size),
                self.tree.camera,
                &self.l.scenes[mode],
                &self.residents[mode],
                &ui,
                false,
            );
        } else if let Some(hg) = &self.hg {
            hg.render(
                &mut self.gpu,
                &view,
                [self.config.width, self.config.height],
                self.dpi,
                viewport(self.size),
                self.state.camera,
                &self.state.matrix,
                mode,
                &self.l.scenes[mode],
                &self.residents[mode],
                &self.state.active,
                self.state.selected,
                &ui,
                false,
            );
        } else {
            self.gpu.render_ui(
                &view,
                [self.config.width, self.config.height],
                self.dpi,
                &ui,
            );
        }
        self.window.pre_present_notify();
        frame.present();
        if args.native_timing.is_some() {
            self.timings.push(serde_json::json!({"mode":if self.tree_mode {"tree"} else {"12d"},"framing":self.state.framing.is_some(),"fitted":self.state.fitted,"outside_files":self.outside_files(),"loading":self.hg.is_none(),"command_seq":self.commands,"plane":self.state.plane,"rotation_t":self.state.t,"playing":self.state.playing,"animating":self.state.animating,"selected":self.state.selected,"tests":self.state.tests,"frame_interval_ms":dt*1000.,"start_lateness_ms":late,"ui_ms":ui_ms,"acquire_ms":acquire_ms,"tick_ms":start.elapsed().as_secs_f64()*1000.}));
        }
        self.frames += 1;
        self.total += 1;
        if self.stats.elapsed().as_secs_f32() >= 0.5 {
            self.state.fps = self.frames as f32 / self.stats.elapsed().as_secs_f32();
            self.frames = 0;
            self.stats = Instant::now();
        }
        anyhow::ensure!(
            self.gpu.errors.lock().unwrap().is_empty(),
            "GPU rendering error"
        );
        Ok(())
    }
    fn loading_ui(&self) -> Vec<Quad> {
        let a = &self.l.atlas;
        let mut out = Vec::new();
        let [w, h] = self.size;
        rect(&mut out, Rect::new(0., 0., w, 64.), rgb(0x111d29), 0.);
        a.text(&mut out, "CODEBUSH", 28., 22., 19., rgb(TEXT), true);
        a.text(
            &mut out,
            "12D SOURCE EXPLORER",
            158.,
            25.,
            11.,
            rgb(MUTED),
            true,
        );
        let x = (w - 500.) / 2.;
        let y = h / 2. - 60.;
        a.text(
            &mut out,
            "Preparing your source view",
            x,
            y,
            25.,
            rgb(TEXT),
            false,
        );
        a.text(
            &mut out,
            &path_label(&self.loading_name, 52),
            x,
            y + 44.,
            14.,
            rgb(ACCENT),
            true,
        );
        for i in 0..12 {
            let phase = self.upload_started.elapsed().as_secs_f32() * 1.8 - i as f32 * 0.25;
            rect(
                &mut out,
                Rect::new(x + i as f32 * 41., y + 82., 31., 3.),
                alpha(rgb(ACCENT), 0.18 + 0.82 * (phase.sin() * 0.5 + 0.5)),
                1.,
            );
        }
        a.text(
            &mut out,
            "Loading all source panels for instant navigation.",
            x,
            y + 108.,
            13.,
            rgb(MUTED),
            false,
        );
        a.text(
            &mut out,
            &format!("{:.1}s", self.upload_started.elapsed().as_secs_f32()),
            x,
            y + 140.,
            12.,
            rgb(MUTED),
            true,
        );
        out
    }
    fn hit_file(&self, p: [f32; 2]) -> Option<usize> {
        self.hg.as_ref()?;
        let world = self.state.camera.world(p, viewport(self.size));
        self.state
            .active
            .iter()
            .filter(|(id, _)| {
                self.hg
                    .as_ref()
                    .unwrap()
                    .bounds(self.state.tests as usize, *id, &self.state.matrix)
                    .inset(-4. / self.state.camera.zoom)
                    .contains(world)
            })
            .min_by(|(a, _), (b, _)| {
                let dist = |id| {
                    let c = self
                        .hg
                        .as_ref()
                        .unwrap()
                        .bounds(self.state.tests as usize, id, &self.state.matrix)
                        .center();
                    (c[0] - world[0]).hypot(c[1] - world[1])
                };
                dist(*a).total_cmp(&dist(*b))
            })
            .map(|x| x.0)
    }
    fn cycle_file(&self, p: [f32; 2]) -> Option<usize> {
        self.hg.as_ref()?;
        let world = self.state.camera.world(p, viewport(self.size));
        let ids: Vec<_> = self
            .state
            .active
            .iter()
            .map(|(id, _)| *id)
            .filter(|&id| {
                self.hg
                    .as_ref()
                    .unwrap()
                    .bounds(self.state.tests as usize, id, &self.state.matrix)
                    .contains(world)
            })
            .collect();
        if ids.is_empty() {
            return None;
        }
        Some(
            ids[self
                .state
                .selected
                .and_then(|id| ids.iter().position(|&i| i == id))
                .map_or(0, |i| (i + 1) % ids.len())],
        )
    }
    fn outside_files(&self) -> usize {
        if self.tree_mode {
            return 0;
        }
        let Some(hg) = &self.hg else {
            return 0;
        };
        let view = self.state.camera.view(viewport(self.size));
        self.state
            .active
            .iter()
            .filter(|&&(id, _)| {
                let b = hg.bounds(self.state.tests as usize, id, &self.state.matrix);
                !view.contains([b.x, b.y]) || !view.contains([b.x + b.w, b.y + b.h])
            })
            .count()
    }
    fn resize_views(&mut self) {
        self.tree.resize(&self.l, self.size, self.dpi);
        if self.state.fitted {
            if let Some(hg) = &self.hg {
                self.state.fit(hg, self.size);
                self.state.camera = self.state.target;
                self.state.framing = None;
            }
        }
    }
    fn save_interaction_state(&self, path: &Path) -> Result<()> {
        if self.hg.is_none() {
            std::fs::write(
                path.with_extension("state.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"loading":true,"name":self.loading_name,"command_seq":self.commands}),
                )?,
            )?;
            return Ok(());
        }
        if self.tree_mode {
            std::fs::write(
                path.with_extension("state.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "mode":"tree", "root":self.l.base.root, "tests":self.tree.state.tests,
                    "logical_size":self.size, "command_seq":self.commands, "resident_bytes":self.bytes,
                    "selected":self.tree.state.selected, "search_matches":self.tree.state.matches.len(),
                    "selected_path":self.tree.state.selected.map(|id| &self.l.base.files[id].path),
                    "active_match":self.tree.state.active_match,
                    "match_path":self.tree.state.matches.get(self.tree.state.active_match).map(|m| &self.l.base.files[m.file].path),
                    "match_line":self.tree.state.matches.get(self.tree.state.active_match).map(|m| m.line),
                    "selected_visible":self.tree.state.selected.and_then(|id| self.l.scenes[self.tree.state.tests as usize].file(id)).is_some_and(|f| f.bounds.intersects(self.tree.camera.view(ui::viewport(self.size)))),
                    "camera_zoom":self.tree.camera.zoom,
                }))?,
            )?;
            return Ok(());
        }
        let vp = viewport(self.size);
        let hg = self.hg.as_ref().unwrap();
        let panels: Vec<_> = self
            .state
            .active
            .iter()
            .map(|&(id, _)| {
                let b = hg.bounds(self.state.tests as usize, id, &self.state.matrix);
                let p = self.state.camera.screen([b.x, b.y], vp);
                (
                    id,
                    Rect::new(
                        p[0],
                        p[1],
                        b.w * self.state.camera.zoom,
                        b.h * self.state.camera.zoom,
                    ),
                )
            })
            .filter(|(_, b)| b.intersects(vp))
            .collect();
        let mut hotspot = None;
        'outer: for (i, (_, a)) in panels.iter().enumerate() {
            for (_, b) in &panels[..i] {
                let x = a.x.max(b.x).max(vp.x);
                let y = a.y.max(b.y).max(vp.y);
                let right = (a.x + a.w).min(b.x + b.w).min(vp.x + vp.w);
                let bottom = (a.y + a.h).min(b.y + b.h).min(vp.y + vp.h);
                if right - x > 2. && bottom - y > 2. {
                    hotspot = Some([(x + right) / 2., (y + bottom) / 2.]);
                    break 'outer;
                }
            }
        }
        let candidates: Vec<_> = hotspot
            .map(|p| {
                panels
                    .iter()
                    .filter(|(_, b)| b.contains(p))
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default();
        let first_reference=self.state.selected.and_then(|id|self.graph.edges.iter().find(|e|
            (e.from==id || e.to==id) && (self.state.tests || !e.test_only) && self.state.weights[e.plane]>1e-6)
            .map(|e|serde_json::json!({"target":if e.from==id {e.to}else{e.from},"source":e.from,"line":e.line})));
        std::fs::write(
            path.with_extension("state.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
            "mode":"12d","tests":self.state.tests,"resident_bytes":self.bytes,"outside_files":self.outside_files(),"framing":self.state.framing.is_some(),"fitted":self.state.fitted,"root":self.l.base.root,"plane":self.state.plane,"t":self.state.t,"selected":self.state.selected,"command_seq":self.commands,"animating":self.state.animating,
                "selected_path":self.state.selected.map(|id|&self.l.base.files[id].path),
                "relations":self.state.relations,"candidate_list":self.state.candidates,
                "hotspot":hotspot,"candidates":candidates,"first_reference":first_reference,
                "logical_size":self.size,"camera_zoom":self.state.camera.zoom,
                "camera_center":self.state.camera.center,"reading":self.state.reading_origin.is_some(),
                "selected_rect":panels.iter().find(|(id,_)|Some(*id)==self.state.selected).map(|(_,b)|b)
            }))?,
        )?;
        Ok(())
    }
}
struct HyperApp {
    args: Args,
    load: Option<(Loaded, Graph, Instant)>,
    running: Option<Live>,
    error: Option<String>,
}
impl ApplicationHandler for HyperApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        let (l, g, s) = self.load.take().unwrap();
        match Live::new(&self.args, l, g, s, el) {
            Ok(r) => {
                r.window.request_redraw();
                self.running = Some(r);
            }
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                el.exit();
            }
        }
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let (Some(path), Some(r)) = (&self.args.native_timing, &self.running) {
            let _=std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"adapter":r.gpu.info.name,"resolution":[r.config.width,r.config.height],"measurement":"CPU native intervals/API timings, not physical displayed frames","rendering_errors":r.gpu.errors.lock().unwrap().clone(),"frames":r.timings})).unwrap());
        }
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(r) = &mut self.running else {
            return;
        };
        if r.tree_mode
            && r.hg.is_some()
            && matches!(
                &event,
                WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::KeyboardInput { .. }
            )
            && !matches!(&event, WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::F12))
        {
            r.tree.modifiers = r.modifiers;
            r.tree.event(&r.l, &event);
            r.cursor = r.tree.cursor;
            if r.state.tests != r.tree.state.tests {
                r.state.tests = r.tree.state.tests;
                r.state.refresh(&r.graph);
                r.state.fit(r.hg.as_ref().unwrap(), r.size);
                r.state.camera = r.state.target;
            }
            if let Some(action) = r.tree.pending.take() {
                r.command(match action {
                    Action::Mode => Command::Mode,
                    Action::Open => Command::Open,
                    Action::Reload => Command::Reload,
                    _ => unreachable!(),
                });
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Focused(false) => {
                r.drag = false;
                r.scrubbing = false;
                r.tree.dragging = false;
            }
            WindowEvent::Resized(s) => {
                if s.width > 0 && s.height > 0 {
                    r.config.width = s.width;
                    r.config.height = s.height;
                    r.size = [s.width as f32 / r.dpi, s.height as f32 / r.dpi];
                    r.surface.configure(&r.gpu.device, &r.config);
                    r.resize_views();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                r.dpi = scale_factor as f32;
                let s = r.window.inner_size();
                r.size = [s.width as f32 / r.dpi, s.height as f32 / r.dpi];
                r.resize_views();
            }
            WindowEvent::ModifiersChanged(m) => r.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let ctrl = r.modifiers.control_key();
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if !r.state.help && !r.state.editing && r.state.candidates.is_none() {
                            r.command(Command::Back);
                        }
                        r.state.help = false;
                        r.state.editing = false;
                        r.state.query.clear();
                        r.state.candidates = None;
                    }
                    Key::Named(NamedKey::Backspace) if r.state.editing => {
                        r.state.query.pop();
                    }
                    Key::Named(NamedKey::Enter) => {
                        r.state.editing = false;
                        r.command(Command::Read);
                    }
                    Key::Named(NamedKey::Space) if !r.state.editing => r.command(Command::Play),
                    Key::Named(NamedKey::F11) => {
                        r.window.set_fullscreen(if r.window.fullscreen().is_some() {
                            None
                        } else {
                            Some(Fullscreen::Borderless(None))
                        })
                    }
                    Key::Named(NamedKey::F12) => {
                        if let Some(path) = &self.args.native_timing {
                            if let Err(e) = r.save_interaction_state(path) {
                                r.state.status = format!("Interaction capture failed: {e:#}");
                            }
                        }
                    }
                    Key::Character(c) => {
                        let c = c.as_str();
                        if ctrl {
                            match c {
                                "o" | "O" => r.command(Command::Open),
                                "r" | "R" => r.command(Command::Reload),
                                "f" | "F" | "k" | "K" => r.command(Command::Search),
                                _ => {}
                            }
                        } else if r.state.editing {
                            if r.state.query.len() < 120 {
                                r.state.query.push_str(c);
                            }
                        } else {
                            match c {
                                "t" | "T" => r.command(Command::Tests),
                                "f" | "F" => r.command(Command::Fit),
                                "[" => r.command(Command::Previous),
                                "]" => r.command(Command::Next),
                                "/" => r.command(Command::Search),
                                "?" => r.command(Command::Help),
                                "o" | "O" => r.command(Command::Overlaps),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = [position.x as f32 / r.dpi, position.y as f32 / r.dpi];
                if r.scrubbing {
                    r.scrub_at(p[0]);
                } else if r.drag {
                    let delta = [p[0] - r.cursor[0], p[1] - r.cursor[1]];
                    r.moved += delta[0].abs() + delta[1].abs();
                    r.state.manual_camera();
                    r.state.target.pan(delta);
                    r.state.camera = r.state.target;
                }
                r.cursor = p;
                r.state.pointer = p;
                r.state.hover_age = 0.;
                r.state.hover_suppressed = false;
                r.window.set_cursor(if r.drag {
                    winit::window::CursorIcon::Grabbing
                } else if r.state.hits.iter().any(|(rect, _)| rect.contains(p)) {
                    winit::window::CursorIcon::Pointer
                } else {
                    winit::window::CursorIcon::Default
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 60.,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if r.cursor[0] < SIDE {
                    r.state.scroll = (r.state.scroll - dy).max(0.);
                } else {
                    r.state.manual_camera();
                    let vp = viewport(r.size);
                    let before = r.state.target.world(r.cursor, vp);
                    r.state.target.zoom =
                        (r.state.target.zoom * (dy * 0.0025).exp()).clamp(0.00001, 10000.);
                    let after = r.state.target.world(r.cursor, vp);
                    for k in 0..2 {
                        r.state.target.center[k] += before[k] - after[k];
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if button == MouseButton::Left || button == MouseButton::Middle =>
            {
                if state == ElementState::Pressed {
                    if r.state.help {
                        r.state.help = false;
                    } else if let Some((_, c)) = r
                        .state
                        .hits
                        .iter()
                        .rev()
                        .find(|(rect, _)| rect.contains(r.cursor))
                    {
                        let c = *c;
                        if matches!(c, Command::Scrub(_)) {
                            r.scrubbing = true;
                            r.scrub_at(r.cursor[0]);
                        } else {
                            r.command(c);
                        }
                    } else if viewport(r.size).contains(r.cursor) {
                        r.drag = true;
                        r.moved = 0.;
                        r.state.editing = false;
                    }
                } else if r.scrubbing {
                    r.scrubbing = false;
                } else if r.drag {
                    r.drag = false;
                    if r.moved < 5. {
                        let id = if r.modifiers.shift_key() {
                            r.cycle_file(r.cursor)
                        } else {
                            r.hit_file(r.cursor)
                        };
                        let double = r.last_click.elapsed() < Duration::from_millis(350)
                            && id == r.state.selected;
                        if let Some(id) = id {
                            r.command(Command::File(id));
                        } else {
                            r.state.selected = None;
                        }
                        r.last_click = Instant::now();
                        if double {
                            r.command(Command::Read);
                        }
                    }
                }
            }
            WindowEvent::DroppedFile(p) => r.load(if p.is_dir() {
                p
            } else {
                p.parent().unwrap().to_owned()
            }),
            WindowEvent::RedrawRequested => {
                if let Err(e) = r.tick(&self.args) {
                    self.error = Some(format!("{e:#}"));
                    el.exit();
                }
                if self.args.smoke_frames.is_some_and(|n| r.total >= n) {
                    el.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(r) = &self.running {
            let now = Instant::now();
            if now >= r.next {
                r.window.request_redraw();
            }
            el.set_control_flow(
                if r.next.saturating_duration_since(now) <= Duration::from_millis(1) {
                    ControlFlow::Poll
                } else {
                    ControlFlow::WaitUntil(r.next - Duration::from_millis(1))
                },
            );
        }
    }
}

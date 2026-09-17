use crate::{
    font::Atlas,
    geometry::*,
    model::{Codebase, language_color},
    spatial::SpatialIndex,
};
use rayon::prelude::*;
use std::{collections::BTreeMap, ops::Range};
pub const CODE_SIZE: f32 = 14.;
pub const LINE_H: f32 = 20.;
pub const COLS: usize = 104;
pub const PAD: f32 = 18.;
pub const HEADER: f32 = 42.;
#[derive(Clone)]
pub struct Chunk {
    pub bounds: Rect,
    pub range: Range<u32>,
    pub file: Option<usize>,
}
#[derive(Clone)]
pub struct FilePlacement {
    pub file: usize,
    pub bounds: Rect,
    pub rows: Vec<RowPosition>,
    pub count: usize,
    pub text_scale: f32,
    pub theme: u8,
}
#[derive(Clone)]
pub struct RowPosition {
    pub line: usize,
    pub x: f32,
    pub y: f32,
    pub chars: usize,
    pub start_col: usize,
}
#[derive(Clone)]
pub struct Directory {
    pub path: String,
    pub bounds: Rect,
    pub depth: usize,
}
pub struct NavRow {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub file: Option<usize>,
}
pub struct Scene {
    pub nav: Vec<NavRow>,
    pub quads: Vec<Quad>,
    pub chunks: Vec<Chunk>,
    pub files: Vec<FilePlacement>,
    pub dirs: Vec<Directory>,
    pub bounds: Rect,
    pub line_count: usize,
    chunk_index: SpatialIndex,
    file_index: SpatialIndex,
    dir_index: SpatialIndex,
}
struct FileMesh {
    quads: Vec<Quad>,
    chunks: Vec<Chunk>,
    placement: FilePlacement,
}
struct FileContent {
    rows: Vec<(usize, usize, Vec<(char, Color)>)>,
    count: usize,
    column_w: f32,
}
#[derive(Clone, Copy, Default)]
struct FileTile {
    bounds: Rect,
    theme: u8,
}
fn source_color(color: Color, light: bool) -> Color {
    // The lexer emits these seven semantic colors. Both surface palettes retain
    // the same syntax distinctions, with contrast appropriate to their paper.
    let role = match (color[0] * 255.).round() as u8 {
        0x99 => 1, // comment
        0xb9 => 2, // string
        0xde => 3, // keyword
        0xf1 => 4, // number
        0xef => 5, // type
        0x9f => 6, // function
        _ => 0,
    };
    rgb(if light {
        [
            0x1c2d3b, 0x485962, 0x245e33, 0x6e2d8c, 0x873f1d, 0x624d14, 0x14566a,
        ][role]
    } else {
        [
            0xf5f7fa, 0xdce4ee, 0xd4f0c7, 0xf0ddfb, 0xfce1bc, 0xfff0b4, 0xc5f0ff,
        ][role]
    })
}
impl FileContent {
    fn area(&self) -> f64 {
        ((self.column_w + PAD * 2.) * (self.rows.len().max(3) as f32 * LINE_H + HEADER + PAD * 2.))
            as f64
    }
}
#[derive(Default)]
struct Node {
    path: String,
    dirs: BTreeMap<String, Node>,
    files: Vec<usize>,
    bounds: Rect,
    weight: f64,
}
impl Node {
    fn insert(&mut self, path: &str, index: usize) {
        if let Some((dir, rest)) = path.split_once('/') {
            let prefix = if self.path.is_empty() {
                dir.to_string()
            } else {
                format!("{}/{dir}", self.path)
            };
            self.dirs
                .entry(dir.to_string())
                .or_insert_with(|| Node {
                    path: prefix,
                    ..Default::default()
                })
                .insert(rest, index);
        } else {
            self.files.push(index);
        }
    }
    fn measure(&mut self, content: &[FileContent]) -> f64 {
        self.weight = self.files.iter().map(|&i| content[i].area()).sum::<f64>()
            + self
                .dirs
                .values_mut()
                .map(|d| d.measure(content))
                .sum::<f64>();
        self.weight.max(1.)
    }
    fn tile(
        &mut self,
        bounds: Rect,
        placements: &mut [FileTile],
        depth: usize,
        phase: u8,
        dirs: &mut Vec<Directory>,
        content: &[FileContent],
    ) {
        self.bounds = bounds;
        dirs.push(Directory {
            path: self.path.clone(),
            bounds,
            depth,
        });
        let inset = bounds.w.min(bounds.h).mul_add(0.008, 0.).clamp(0.1, 18.);
        let header = (bounds.h * 0.055).min(58.).max(inset);
        let inner = Rect::new(
            bounds.x + inset,
            bounds.y + header,
            bounds.w - inset * 2.,
            bounds.h - header - inset,
        );
        let mut children: Vec<(bool, usize, f64, Rect)> = self
            .dirs
            .values()
            .enumerate()
            .map(|(i, d)| (true, i, d.weight, Rect::default()))
            .chain(
                self.files
                    .iter()
                    .map(|&i| (false, i, content[i].area(), Rect::default())),
            )
            .collect();
        children.sort_by(|a, b| b.2.total_cmp(&a.2));
        if children.is_empty() {
            return;
        }
        // Squarify in a stretched metric so leaf panels favor tall code columns.
        // The inverse transform still exactly tiles the available directory area.
        const TALL: f64 = 2.;
        streemap::squarify(
            streemap::Rect {
                x: 0.,
                y: 0.,
                w: inner.w as f64 * TALL,
                h: inner.h as f64,
            },
            &mut children,
            |c| c.2,
            |c, r| {
                c.3 = Rect::new(
                    inner.x + (r.x / TALL) as f32,
                    inner.y + r.y as f32,
                    (r.w / TALL) as f32,
                    r.h as f32,
                );
            },
        );
        let mut directory_bounds = vec![(Rect::default(), 0u8); self.dirs.len()];
        for (order, (is_dir, i, _, r)) in children.into_iter().enumerate() {
            let gap = (r.w.min(r.h) * 0.015).min(6.);
            let r = r.inset(gap);
            let theme = ((phase as usize + order) % 4) as u8;
            if is_dir {
                directory_bounds[i] = (r, theme);
            } else {
                placements[i] = FileTile { bounds: r, theme };
            }
        }
        for (dir, (bounds, theme)) in self.dirs.values_mut().zip(directory_bounds) {
            dir.tile(bounds, placements, depth + 1, theme, dirs, content);
        }
    }
}
fn file_content(base: &Codebase, id: usize, atlas: &Atlas, tests: bool) -> FileContent {
    let f = &base.files[id];
    let advance = atlas.advance('M', CODE_SIZE, false);
    let mut display: Vec<(usize, usize, Vec<(char, Color)>)> = Vec::new();
    let mut count = 0;
    for line in &f.lines {
        if !tests && f.hidden_lines.contains(&line.number) {
            continue;
        }
        count += 1;
        let mut chars = Vec::new();
        for span in &line.spans {
            for c in span.text.chars() {
                if c == '\t' {
                    let n = 4 - chars.len() % 4;
                    chars.extend(std::iter::repeat_n((' ', span.color), n));
                } else if atlas.glyphs.contains_key(&(false, c)) {
                    chars.push((c, span.color));
                } else {
                    chars.extend(
                        format!("\\u{{{:X}}}", c as u32)
                            .chars()
                            .map(|ch| (ch, span.color)),
                    );
                }
            }
        }
        if chars.is_empty() {
            display.push((line.number, 0, vec![]));
        } else {
            for (i, row) in chars.chunks(COLS).enumerate() {
                display.push((line.number, i * COLS, row.to_vec()));
            }
        }
    }
    let width_chars = display
        .iter()
        .map(|r| r.2.len())
        .max()
        .unwrap_or(0)
        .clamp(42, COLS);
    let column_w = width_chars as f32 * advance + 70.;
    FileContent {
        rows: display,
        count,
        column_w,
    }
}
fn make_file(
    base: &Codebase,
    id: usize,
    atlas: &Atlas,
    content: FileContent,
    tile: FileTile,
) -> FileMesh {
    let f = &base.files[id];
    let target = tile.bounds;
    let light = tile.theme % 2 == 0;
    let ink = if light { 0x1c2d3b } else { 0xf5f7fa };
    let secondary = if light { 0x485962 } else { 0xdce4ee };
    let FileContent {
        rows: display,
        count,
        column_w,
    } = content;
    let advance = atlas.advance('M', CODE_SIZE, false);
    let n = display.len().max(3);
    // Choose the number of continuation columns that gives the largest uniform
    // text size in this tile. No source is truncated or stretched nonuniformly.
    let estimate = ((n as f32 * LINE_H * target.w / (column_w * target.h)).sqrt() as usize).max(1);
    let mut choices = vec![1];
    choices.extend(estimate.saturating_sub(3).max(1)..=estimate.saturating_add(4).min(n));
    let (columns, column_rows, scale) = choices
        .into_iter()
        .map(|columns| {
            let rows = n.div_ceil(columns);
            let w = column_w * columns as f32 + 28. * (columns - 1) as f32 + PAD * 2.;
            let h = HEADER + PAD * 2. + if columns > 1 { 24. } else { 0. } + rows as f32 * LINE_H;
            (columns, rows, (target.w / w).min(target.h / h))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .unwrap();
    let column_gap = 28.;
    let body_y = HEADER + PAD + if columns > 1 { 24. } else { 0. };
    let w = target.w / scale;
    let h = target.h / scale;
    let bounds = Rect::new(0., 0., w, h);
    let mut quads = Vec::new();
    let mut chunks = Vec::new();
    let mut row_positions = Vec::new();
    rect(&mut quads, bounds, rgb(0x080e16), 2.);
    rect(
        &mut quads,
        bounds.inset(2.),
        rgb(file_background(tile.theme)),
        1.,
    );
    rect(
        &mut quads,
        Rect::new(1., 1., w - 2., HEADER),
        rgb(0x243544),
        3.,
    );
    rect(
        &mut quads,
        Rect::new(0., 0., 3., h),
        language_color(f.language),
        1.,
    );
    atlas.middle_text(
        &mut quads,
        f.path.rsplit('/').next().unwrap(),
        PAD,
        10.,
        16.,
        rgb(TEXT),
        w - PAD * 2.,
    );
    let label = format!(
        "{}  ·  {} lines{}",
        f.language,
        count,
        if f.is_test { "  ·  TEST" } else { "" }
    );
    let lw = atlas.text_width(&label, 12., true);
    if w > atlas.text_width(f.path.rsplit('/').next().unwrap(), 16., true) + lw + 60. {
        atlas.text(
            &mut quads,
            &label,
            w - lw - PAD,
            13.,
            12.,
            language_color(f.language),
            true,
        );
    }
    chunks.push(Chunk {
        bounds,
        range: 0..quads.len() as u32,
        file: Some(id),
    });
    if columns > 1 {
        let begin = quads.len() as u32;
        for col in 0..columns {
            let first = col * column_rows;
            if first >= display.len() {
                break;
            }
            let last = (first + column_rows).min(display.len()) - 1;
            let x = PAD + col as f32 * (column_w + column_gap);
            atlas.text(
                &mut quads,
                &format!("{} - {}", display[first].0, display[last].0),
                x + 52.,
                HEADER + PAD,
                10.,
                rgb(secondary),
                true,
            );
            if col > 0 {
                rect(
                    &mut quads,
                    Rect::new(
                        x - column_gap * 0.5,
                        HEADER + PAD,
                        1.,
                        h - HEADER - PAD * 2.,
                    ),
                    rgb(if light { 0x82929e } else { 0xb4c4d4 }),
                    0.,
                );
            }
        }
        chunks.push(Chunk {
            bounds,
            range: begin..quads.len() as u32,
            file: Some(id),
        });
    }
    for (block, rows) in display.chunks(32).enumerate() {
        // Keep source blocks small for precise culling when reading a tall file.
        let begin = quads.len() as u32;
        let mut bx = f32::MAX;
        let mut by = f32::MAX;
        let mut ex: f32 = 0.;
        let mut ey: f32 = 0.;
        for (j, (line, start_col, chars)) in rows.iter().enumerate() {
            let row = block * 32 + j;
            let x = PAD + (row / column_rows) as f32 * (column_w + column_gap);
            let y = body_y + (row % column_rows) as f32 * LINE_H;
            let line_label = if *start_col == 0 {
                line.to_string()
            } else {
                "↳".to_string()
            };
            let nw = atlas.text_width(&line_label, 11., false);
            atlas.text(
                &mut quads,
                &line_label,
                x + 37. - nw,
                y + 2.,
                11.,
                rgb(secondary),
                false,
            );
            let tx = x + 52.;
            for (k, (c, color)) in chars.iter().enumerate() {
                atlas.text(
                    &mut quads,
                    &c.to_string(),
                    tx + k as f32 * advance,
                    y,
                    CODE_SIZE,
                    source_color(*color, light),
                    false,
                );
            }
            row_positions.push(RowPosition {
                line: *line,
                x: tx,
                y,
                chars: chars.len(),
                start_col: *start_col,
            });
            bx = bx.min(x);
            by = by.min(y);
            ex = ex.max(x + column_w);
            ey = ey.max(y + LINE_H);
        }
        chunks.push(Chunk {
            bounds: Rect::new(bx, by, ex - bx, ey - by),
            range: begin..quads.len() as u32,
            file: Some(id),
        });
    }
    if display.is_empty() {
        atlas.text(
            &mut quads,
            if count == 0 && !f.lines.is_empty() {
                "All lines are test-only"
            } else {
                "Empty source file"
            },
            PAD + 52.,
            HEADER + PAD,
            CODE_SIZE,
            rgb(ink),
            false,
        );
        chunks.push(Chunk {
            bounds,
            range: chunks.last().map_or(0, |c| c.range.end)..quads.len() as u32,
            file: Some(id),
        });
    }
    for q in &mut quads {
        q.rect[0] = target.x + q.rect[0] * scale;
        q.rect[1] = target.y + q.rect[1] * scale;
        q.rect[2] *= scale;
        q.rect[3] *= scale;
        if q.uv[0] < 0. {
            q.uv[1] *= scale;
        }
    }
    for c in &mut chunks {
        c.bounds = Rect::new(
            target.x + c.bounds.x * scale,
            target.y + c.bounds.y * scale,
            c.bounds.w * scale,
            c.bounds.h * scale,
        );
    }
    for row in &mut row_positions {
        row.x = target.x + row.x * scale;
        row.y = target.y + row.y * scale;
    }
    FileMesh {
        quads,
        chunks,
        placement: FilePlacement {
            file: id,
            bounds: target,
            rows: row_positions,
            count,
            text_scale: scale,
            theme: tile.theme,
        },
    }
}
impl Scene {
    pub fn build(base: &Codebase, atlas: &Atlas, tests: bool) -> Self {
        let ids: Vec<usize> = base
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| tests || !f.is_test)
            .map(|(i, _)| i)
            .collect();
        let content: Vec<FileContent> = ids
            .par_iter()
            .map(|&id| file_content(base, id, atlas, tests))
            .collect();
        let mut root = Node::default();
        for (i, &id) in ids.iter().enumerate() {
            root.insert(&base.files[id].path, i);
        }
        let area = root.measure(&content).max(400_000.) * 1.2;
        let bounds = Rect::new(
            0.,
            0.,
            (area * 1.65).sqrt() as f32,
            (area / 1.65).sqrt() as f32,
        );
        let mut placements = vec![FileTile::default(); ids.len()];
        let mut dirs = Vec::new();
        root.tile(bounds, &mut placements, 0, 0, &mut dirs, &content);
        // Tiny files need boundaries, not alternating white flashes across a
        // dense directory. Keep their two ink surfaces distinct, while larger
        // panels retain the full paper/ink palette. This is chosen once so
        // colors remain stable as the camera moves.
        for tile in &mut placements {
            if tile.bounds.w as f64 * (tile.bounds.h as f64) < area * 0.00015 {
                tile.theme = 1 + (tile.theme % 2) * 2;
            }
        }
        let meshes: Vec<FileMesh> = content
            .into_par_iter()
            .enumerate()
            .map(|(i, content)| make_file(base, ids[i], atlas, content, placements[i]))
            .collect();
        let mut quads = Vec::new();
        let mut chunks = Vec::new();
        for d in &dirs {
            let start = quads.len() as u32;
            let c = directory_color(&d.path);
            rect(&mut quads, d.bounds, c, 4.);
            let border = d.bounds.w.min(d.bounds.h).mul_add(0.006, 0.).min(3.);
            rect(&mut quads, d.bounds.inset(border), rgb(0x111b24), 3.);
            let name = if d.path.is_empty() {
                base.name.as_str()
            } else {
                d.path.rsplit('/').next().unwrap()
            };
            let header = (d.bounds.h * 0.055).min(58.);
            let size = (header * 0.36).min(20.);
            rect(
                &mut quads,
                Rect::new(d.bounds.x, d.bounds.y, d.bounds.w, header * 0.9),
                c,
                0.,
            );
            atlas.middle_text(
                &mut quads,
                &format!("{name}/"),
                d.bounds.x + border * 3.,
                d.bounds.y + header * 0.25,
                size,
                rgb(0x07131d),
                (d.bounds.w - border * 6.).max(0.),
            );
            chunks.push(Chunk {
                bounds: d.bounds,
                range: start..quads.len() as u32,
                file: None,
            });
        }
        let mut files = Vec::new();
        let mut line_count = 0;
        for mut m in meshes {
            let off = quads.len() as u32;
            for c in &mut m.chunks {
                c.range.start += off;
                c.range.end += off;
            }
            line_count += m.placement.count;
            quads.append(&mut m.quads);
            chunks.append(&mut m.chunks);
            files.push(m.placement);
        }
        files.sort_by_key(|f| f.file);
        let mut nav = Vec::new();
        let mut previous: Vec<String> = Vec::new();
        for placement in &files {
            let path = &base.files[placement.file].path;
            let parts: Vec<_> = path.split('/').collect();
            let dirs = &parts[..parts.len() - 1];
            for depth in 0..dirs.len() {
                let dir = dirs[..=depth].join("/");
                if previous.get(depth) != Some(&dir) {
                    nav.push(NavRow {
                        path: dir,
                        name: dirs[depth].into(),
                        depth,
                        file: None,
                    });
                }
            }
            previous = (0..dirs.len()).map(|n| dirs[..=n].join("/")).collect();
            nav.push(NavRow {
                path: path.clone(),
                name: parts.last().unwrap().to_string(),
                depth: dirs.len(),
                file: Some(placement.file),
            });
        }
        Self {
            chunk_index: SpatialIndex::new(chunks.iter().map(|c| c.bounds)),
            file_index: SpatialIndex::new(files.iter().map(|f| f.bounds)),
            dir_index: SpatialIndex::new(dirs.iter().map(|d| d.bounds)),
            nav,
            quads,
            chunks,
            files,
            dirs,
            bounds: root.bounds,
            line_count,
        }
    }
    pub fn file(&self, id: usize) -> Option<&FilePlacement> {
        self.files
            .binary_search_by_key(&id, |f| f.file)
            .ok()
            .map(|i| &self.files[i])
    }
    pub fn layout_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "files": self.files.len(), "directories": self.dirs.len(), "bounds": self.bounds,
            "file_area_fraction": self.files.iter().map(|f| f.bounds.w as f64 * f.bounds.h as f64).sum::<f64>()
                / (self.bounds.w as f64 * self.bounds.h as f64),
            "text_scale_min": self.files.iter().map(|f| f.text_scale).reduce(f32::min),
            "text_scale_max": self.files.iter().map(|f| f.text_scale).reduce(f32::max),
        })
    }
    pub fn visible_chunks(&self, view: Rect) -> Vec<usize> {
        self.chunk_index.query(view)
    }
    pub fn visible_files(&self, view: Rect) -> Vec<usize> {
        self.file_index.query(view)
    }
    pub fn visible_dirs(&self, view: Rect) -> Vec<usize> {
        self.dir_index.query(view)
    }
    pub fn hit(&self, p: [f32; 2]) -> Option<usize> {
        self.files
            .iter()
            .find(|f| f.bounds.contains(p))
            .map(|f| f.file)
    }
}

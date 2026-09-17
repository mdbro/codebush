use codebush::{
    camera::Camera,
    font::Atlas,
    geometry::Rect,
    model::{Codebase, language, test_path},
    scene::Scene,
    ui,
};
use std::path::Path;
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "codebush-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn put(&self, path: &str, code: &str) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, code).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn source_allowlist_and_test_names() {
    assert_eq!(language(Path::new("code.rs")), Some("Rust"));
    for p in [
        "readme.md",
        "config.json",
        "image.png",
        "Cargo.toml",
        "file.mp4",
    ] {
        assert_eq!(language(Path::new(p)), None);
    }
    for p in [
        "tests/unit.rs",
        "src/tests.rs",
        "src/test.py",
        "src/spec.ts",
        "test_alpha.py",
        "a/foo.test.ts",
        "src/FooTests.java",
        "x_test.go",
        "spec/a.rb",
    ] {
        assert!(test_path(p), "{p}");
    }
    for p in [
        "contest.rs",
        "latest.py",
        "src/testament.rs",
        "src/prod_test_utils.rs",
    ] {
        assert!(!test_path(p), "{p}");
    }
}
#[test]
fn source_fidelity_test_toggle_and_layout() {
    let f = Fixture::new();
    let production = "pub fn café() -> i32 { 42 }\n#[cfg(any(test, feature = \"keep\"))]\npub fn shared() {}\n#[cfg(test)]\nmod tests {\n #[test]\n fn check() { assert_eq!(super::café(), 42); }\n}\n";
    f.put("src/lib.rs", production);
    f.put(
        "src/long.py",
        &format!(
            "\tvalue = '{}'\n{}\n",
            "λ".repeat(235),
            "# detail\n".repeat(450)
        ),
    );
    f.put("tests/integration.rs", "#[test]\nfn integration() {}\n");
    f.put("README.md", "not source");
    f.put("target/generated.rs", "excluded");
    f.put("node_modules/dep.js", "excluded");
    f.put("nested/run.sh", "#!/bin/sh\necho hi\n");
    f.put(
        ".github/scripts/check.py",
        "print('source in a hidden directory')\n",
    );
    f.put(".gitignore", "ignored.py\n");
    f.put("ignored.py", "excluded");
    let base = Codebase::scan(&f.0).unwrap();
    assert_eq!(base.files.len(), 5);
    let lib = base.files.iter().find(|f| f.path == "src/lib.rs").unwrap();
    assert_eq!(lib.text, production);
    assert!(lib.hidden_lines.contains(&4));
    assert!(lib.hidden_lines.contains(&8));
    assert!(!lib.hidden_lines.contains(&2));
    let atlas = Atlas::new(base.chars());
    let on = Scene::build(&base, &atlas, true);
    let off = Scene::build(&base, &atlas, false);
    assert_eq!(on.files.len(), 5);
    assert_eq!(off.files.len(), 4);
    assert_eq!(on.line_count, base.stats.lines);
    assert_eq!(off.line_count, base.stats.lines - 7);
    for scene in [&on, &off] {
        for a in &scene.files {
            for b in &scene.files {
                if a.file != b.file {
                    assert!(!a.bounds.intersects(b.bounds), "files overlap");
                }
            }
            let source = &base.files[a.file];
            for rows in a.rows.windows(2) {
                assert!(
                    (rows[1].line, rows[1].start_col) > (rows[0].line, rows[0].start_col),
                    "source order must survive wrapping and column continuation"
                );
                assert!(
                    (rows[1].x == rows[0].x && rows[1].y > rows[0].y)
                        || (rows[1].x > rows[0].x && rows[1].y < rows[0].y),
                    "read down each column, then continue in the next column"
                );
            }
            if source.path == "src/long.py" {
                assert!(
                    a.bounds.h < a.bounds.w * 6.,
                    "a long file must not force a needle-shaped overview"
                );
                assert!(
                    a.rows.last().unwrap().x > a.rows[0].x,
                    "long source must continue into another visible column"
                );
            }
            for line in &source.lines {
                if std::ptr::eq(scene, &off) && source.hidden_lines.contains(&line.number) {
                    continue;
                }
                assert!(
                    a.rows.iter().any(|r| r.line == line.number),
                    "lost line {}",
                    line.number
                );
            }
            for r in &a.rows {
                assert!(a.bounds.contains([r.x, r.y]));
                assert!(a.bounds.contains([
                    r.x + r.chars as f32 * atlas.advance('M', 14., false) * a.text_scale,
                    r.y + 20. * a.text_scale
                ]));
            }
        }
        for chunk in &scene.chunks {
            assert!(chunk.range.end as usize <= scene.quads.len());
        }
        // Culling must retain every intersecting primitive, in paint order,
        // across overview, edge-crossing and close reading viewports.
        for yi in -1..11 {
            for xi in -1..11 {
                for scale in [0.01, 0.15, 0.6, 2.] {
                    let view = Rect::new(
                        xi as f32 * scene.bounds.w / 10.,
                        yi as f32 * scene.bounds.h / 10.,
                        scene.bounds.w * scale,
                        scene.bounds.h * scale,
                    );
                    let expected: Vec<_> = scene
                        .chunks
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.bounds.intersects(view))
                        .map(|(i, _)| i)
                        .collect();
                    assert_eq!(scene.visible_chunks(view), expected);
                    let expected: Vec<_> = scene
                        .files
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.bounds.intersects(view))
                        .map(|(i, _)| i)
                        .collect();
                    assert_eq!(scene.visible_files(view), expected);
                    let expected: Vec<_> = scene
                        .dirs
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| d.bounds.intersects(view))
                        .map(|(i, _)| i)
                        .collect();
                    assert_eq!(scene.visible_dirs(view), expected);
                }
            }
        }
    }
    assert!(ui::search(&base, "assert_eq", false).is_empty());
    assert_eq!(ui::search(&base, "assert_eq", true).len(), 1);
    let hits = ui::search(&base, "detail", true);
    assert!(
        hits.windows(2)
            .all(|pair| (pair[0].file, pair[0].line) < (pair[1].file, pair[1].line))
    );
}
#[test]
fn zoom_keeps_pointer_world_position() {
    let vp = Rect::new(270., 116., 1650., 934.);
    let mut c = Camera::default();
    c.fit(Rect::new(0., 0., 10000., 8000.), vp);
    let p = [782., 550.];
    let before = c.world(p, vp);
    c.zoom_at(2.73, p, vp);
    let after = c.world(p, vp);
    assert!((before[0] - after[0]).abs() < 0.001);
    assert!((before[1] - after[1]).abs() < 0.001);
}

#[test]
fn malformed_text_is_preserved_as_visible_escapes() {
    let f = Fixture::new();
    f.put("src/nul.rs", "let nul = \"\0\";\n");
    std::fs::write(f.0.join("src/bytes.rs"), b"// byte: \xff\nfn main() {}\n").unwrap();
    let base = Codebase::scan(&f.0).unwrap();
    assert_eq!(base.files.len(), 2);
    assert_eq!(base.stats.warnings.len(), 1);
    assert!(base.files.iter().any(|f| f.text.contains("\\xFF")));
    assert!(base.files.iter().any(|f| f.text.contains('\0')));
    let atlas = Atlas::new(base.chars());
    let scene = Scene::build(&base, &atlas, true);
    assert_eq!(scene.files.len(), 2);
}

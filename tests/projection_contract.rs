use codebush::{model::Codebase, projection::*, references::Graph};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "codebush-12d-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn put(root: &PathBuf, path: &str, text: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}
fn distance(a: Vector, b: Vector) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum()
}
fn apply(m: &Matrix, p: Vector) -> Vector {
    std::array::from_fn(|i| dot(m[i], p))
}
fn determinant(mut m: Matrix) -> f64 {
    let mut d = 1.;
    for i in 0..DIM {
        let p = (i..DIM)
            .max_by(|&a, &b| m[a][i].abs().total_cmp(&m[b][i].abs()))
            .unwrap();
        if p != i {
            m.swap(p, i);
            d = -d;
        }
        let v = m[i][i];
        d *= v;
        if v.abs() < 1e-14 {
            return 0.;
        }
        for j in i + 1..DIM {
            let f = m[j][i] / v;
            for k in i + 1..DIM {
                m[j][k] -= f * m[i][k];
            }
        }
    }
    d
}
#[test]
fn all_66_coordinate_planes_are_exact() {
    let v: Vector = std::array::from_fn(|i| (i * i + 3) as f64);
    for (p, &[i, j]) in PAIRS.iter().enumerate() {
        let r = frame(p);
        let xy = project(&r, v);
        assert!((xy[0] as f64 - v[i]).abs() < 1e-5);
        assert!((xy[1] as f64 - v[j]).abs() < 1e-5);
        assert!(orthogonality_error(&r) < 1e-12);
        assert!((determinant(r) - 1.).abs() < 1e-12);
        let w = weights(&r);
        for k in 0..PLANES {
            assert!((w[k] - if k == p { 1. } else { 0. }).abs() < 1e-6);
        }
    }
}
#[test]
fn every_plane_transition_preserves_rigid_geometry_and_endpoints() {
    let a: Vector = std::array::from_fn(|i| ((i * 7) as f64).sin() * 100.);
    let b: Vector = std::array::from_fn(|i| ((i * 3) as f64).cos() * 40.);
    for from in 0..PLANES {
        for to in 0..PLANES {
            let rot = Rotation::new(frame(from), frame(to));
            for t in [0.00001, 0.17, 0.5, 0.83, 0.99999] {
                let r = rot.at(t);
                assert!(orthogonality_error(&r) < 1e-12);
                assert!((distance(apply(&r, a), apply(&r, b)) - distance(a, b)).abs() < 1e-7);
                assert!((determinant(r) - 1.).abs() < 1e-11);
                let w = weights(&r);
                assert!((w.iter().sum::<f32>() - 1.).abs() < 1e-5);
                assert!(w.iter().all(|&x| x >= 0.));
            }
            let almost = rot.at(1. - 1e-7);
            for i in 0..DIM {
                for j in 0..DIM {
                    assert!(
                        (almost[i][j] - rot.target[i][j]).abs() < 1e-10,
                        "endpoint jump {from}->{to}"
                    );
                }
            }
        }
    }
}
#[test]
fn interrupted_rotation_keeps_geometry_and_starts_continuously() {
    let mid = Rotation::new(frame(8), frame(42)).at(0.37);
    let next = Rotation::new(mid, frame(65));
    for i in 0..DIM {
        for j in 0..DIM {
            assert!((next.at(1e-7)[i][j] - mid[i][j]).abs() < 1e-10);
            assert!((next.at(1. - 1e-7)[i][j] - frame(65)[i][j]).abs() < 1e-10);
        }
    }
}
#[test]
fn directed_references_have_evidence_and_every_edge_appears_in_one_plane() {
    let p = fixture();
    put(
        &p,
        "src/lib.rs",
        "mod alpha; mod beta; #[cfg(test)] mod tests;\n",
    );
    put(
        &p,
        "src/alpha.rs",
        "use crate::beta::B;\npub fn f() { let _ = crate::beta::B; }\n// use crate::tests::x;\n",
    );
    put(&p, "src/beta.rs", "pub struct B;\n");
    put(
        &p,
        "src/tests.rs",
        "use crate::alpha::f; #[test] fn t(){f();}\n",
    );
    put(
        &p,
        "web/a.ts",
        "import { b } from './b';\n// import './fake';\nconst s = \"import './fake'\";\n",
    );
    put(&p, "web/b.ts", "export const b = 2;\n");
    put(&p, "web/fake.ts", "export const fake = 1;\n");
    put(
        &p,
        "native/a.cpp",
        "#include \"b.h\"\n// #include \"fake.h\"\n",
    );
    put(&p, "native/b.h", "int f();\n");
    put(&p, "README.md", "not source");
    let b = Codebase::scan(&p).unwrap();
    let g = Graph::build(&b);
    let id = |s: &str| b.files.iter().position(|f| f.path == s).unwrap();
    for (a, z) in [
        ("src/lib.rs", "src/alpha.rs"),
        ("src/lib.rs", "src/beta.rs"),
        ("src/alpha.rs", "src/beta.rs"),
        ("src/tests.rs", "src/alpha.rs"),
        ("web/a.ts", "web/b.ts"),
        ("native/a.cpp", "native/b.h"),
    ] {
        assert!(
            g.edges.iter().any(|e| e.from == id(a) && e.to == id(z)),
            "missing {a}->{z}"
        );
    }
    assert!(!g.edges.iter().any(|e| e.to == id("web/fake.ts")));
    for (i, e) in g.edges.iter().enumerate() {
        assert!(e.line > 0);
        assert_eq!(
            g.plane_edges[1]
                .iter()
                .filter(|ids| ids.contains(&i))
                .count(),
            1
        );
        assert_eq!(
            g.plane_edges[0]
                .iter()
                .filter(|ids| ids.contains(&i))
                .count(),
            if e.test_only { 0 } else { 1 }
        );
    }
    for mode in 0..2 {
        for plane in 0..PLANES {
            let active = g.active(mode, &weights(&frame(plane)));
            let expected: std::collections::BTreeSet<_> = g.plane_edges[mode][plane]
                .iter()
                .flat_map(|&i| [g.edges[i].from, g.edges[i].to])
                .collect();
            assert_eq!(
                active
                    .iter()
                    .map(|(i, _)| *i)
                    .collect::<std::collections::BTreeSet<_>>(),
                expected
            );
        }
    }
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn imports_survive_new_syntax_and_follow_declared_parent_ownership() {
    let p = fixture();
    put(
        &p,
        "src/lib.rs",
        "pub struct Library; mod glob; pub struct Candidate; pub enum ErrorKind {} pub struct Error; fn new_regex() {} // fn Ghost() {}\n",
    );
    put(
        &p,
        "src/glob.rs",
        "use crate::{Candidate, Error, ErrorKind, new_regex}; use crate::Ghost;\n",
    );
    put(
        &p,
        "src/main.rs",
        "mod child; mod recover; mod beta; pub struct Owner;\n",
    );
    put(
        &p,
        "src/child.rs",
        "use super::*; use super::Owner; use super::Ghost;\n",
    );
    // Balanced tokens but an invalid/new AST item must not discard a valid import.
    put(
        &p,
        "src/recover.rs",
        "use crate::beta::B; fn f() { let x = ; }\n",
    );
    put(&p, "src/beta.rs", "pub struct B;\n");
    let b = Codebase::scan(&p).unwrap();
    let g = Graph::build(&b);
    let has = |a: &str, z: &str| {
        g.edges
            .iter()
            .any(|e| b.files[e.from].path == a && b.files[e.to].path == z)
    };
    assert!(has("src/child.rs", "src/main.rs"));
    assert!(!has("src/child.rs", "src/lib.rs"));
    assert!(has("src/recover.rs", "src/beta.rs"));
    assert!(has("src/glob.rs", "src/lib.rs"));
    assert!(!has("src/glob.rs", "src/main.rs"));
    assert!(g.unresolved.iter().any(|u| u.spelling == "crate::Ghost"));
    assert!(g.parse_failures.contains(&"src/recover.rs".into()));
    assert!(g.unresolved.iter().any(|u| u.spelling == "super::Ghost"));
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn language_comments_and_property_calls_do_not_invent_links() {
    let p = fixture();
    put(
        &p,
        "js/a.ts",
        "Buffer.from('./fake'); obj.require('./fake'); import './real';\n",
    );
    put(&p, "js/fake.ts", "export const fake = 1;\n");
    put(&p, "js/real.ts", "export const real = 1;\n");
    put(&p, "sh/a.sh", "# source './fake.sh'\nsource './real.sh'\n");
    put(&p, "sh/fake.sh", "echo fake\n");
    put(&p, "sh/real.sh", "echo real\n");
    put(
        &p,
        "rb/a.rb",
        "# require './fake'\nrequire_relative './real'\n",
    );
    put(&p, "rb/fake.rb", "puts 'fake'\n");
    put(&p, "rb/real.rb", "puts 'real'\n");
    put(
        &p,
        "lua/a.lua",
        "-- require './fake'\n--[[ require './fake' ]]\nlocal s = [[require './fake']]\nrequire './real'\n",
    );
    put(&p, "lua/fake.lua", "return 1\n");
    put(&p, "lua/real.lua", "return 2\n");
    let b = Codebase::scan(&p).unwrap();
    let g = Graph::build(&b);
    assert_eq!(g.edges.len(), 4, "{:?}", g.edges);
    assert!(g.edges.iter().all(|e| b.files[e.to].path.contains("real.")));
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn cargo_declared_test_target_scopes_sibling_and_nested_modules() {
    let p = fixture();
    put(
        &p,
        "Cargo.toml",
        "[package]\nname='scope-fixture'\nversion='0.1.0'\nautotests=false\n[[test]]\nname='integration'\npath='checks/suite.rs'\n",
    );
    put(&p, "src/lib.rs", "pub struct Production;\n");
    put(&p, "checks/suite.rs", "mod util; mod index;\n");
    put(&p, "checks/util.rs", "pub struct Dir;\n");
    put(&p, "checks/index/mod.rs", "mod basic;\n");
    put(&p, "checks/index/basic.rs", "use crate::util::Dir;\n");
    let b = Codebase::scan(&p).unwrap();
    let g = Graph::build(&b);
    let has = |a: &str, z: &str| {
        g.edges
            .iter()
            .any(|e| b.files[e.from].path == a && b.files[e.to].path == z)
    };
    assert!(has("checks/suite.rs", "checks/util.rs"));
    assert!(has("checks/suite.rs", "checks/index/mod.rs"));
    assert!(has("checks/index/basic.rs", "checks/util.rs"));
    assert!(
        b.files
            .iter()
            .find(|f| f.path == "checks/suite.rs")
            .unwrap()
            .is_test
    );
    assert!(g.manifest_failures.is_empty());
    assert!(g.edges.iter().all(|e| e.test_only));
    assert!(g.plane_edges[0].iter().all(|e| e.is_empty()));
    assert!(!b.files.iter().any(|f| f.path == "Cargo.toml"));
    fs::remove_dir_all(p).unwrap();
}

#[test]
fn rotation_framing_contains_continuous_paths_and_tall_panels() {
    use codebush::{camera::Camera, geometry::Rect};
    // Deliberately asymmetric, far from the origin, and absent from endpoint-only
    // visibility in some planes. An origin-centred endpoint fit cannot cover this.
    let files = vec![
        (
            [
                9000., 3000., -18000., 700., 1., -5000., 90., 23000., 600., -70., 8200., -9900.,
            ],
            [43., 720.],
        ),
        (
            [
                -2000., 19000., 12., 9800., 7500., 1800., -6000., 800., -7300., 16500., 0., 3900.,
            ],
            [720., 180.],
        ),
    ];
    for from in 0..PLANES {
        for to in 0..PLANES {
            let rotation = Rotation::new(frame(from), frame(to));
            // Include a rotation interrupted partway and redirected to another pair.
            let redirected = Rotation::new(rotation.at(0.417), frame((to + 23) % PLANES));
            for r in [&rotation, &redirected] {
                let bounds = r.swept_bounds(&files);
                for size in [[1200., 850.], [1760., 1080.], [7680., 4320.]] {
                    let vp = Rect::new(310., 126., size[0] - 310., size[1] - 180.);
                    let mut camera = Camera::default();
                    camera.fit(bounds, vp);
                    for i in 0..=19 {
                        let m = r.at(i as f64 / 19.);
                        for &(p, wh) in &files {
                            let p = project(&m, p);
                            for sign in [-0.5, 0.5] {
                                assert!(
                                    vp.contains(
                                        camera
                                            .screen([p[0] + sign * wh[0], p[1] + sign * wh[1]], vp)
                                    ),
                                    "escaped {from}->{to} at {i}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

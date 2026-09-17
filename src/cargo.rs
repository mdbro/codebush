//! Cargo target metadata. Manifests describe scope but never become visible source.
use rayon::prelude::*;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Target {
    pub path: String,
    pub name: String,
    pub library: bool,
    pub test: bool,
}
#[derive(Default)]
pub struct Targets {
    pub entries: Vec<Target>,
    pub errors: Vec<String>,
}
fn joined(dir: &str, path: &str) -> String {
    let joined = format!("{dir}/{path}").replace('\\', "/");
    let mut parts = vec![];
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}
pub fn read(root: &Path, files: &BTreeSet<String>, manifests: &[PathBuf]) -> Targets {
    let results: Vec<_> = manifests
        .par_iter()
        .map(|path| -> Result<Vec<Target>, String> {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let doc = text
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let Some(package) = doc.get("package") else {
                return Ok(vec![]);
            };
            let dir = path
                .parent()
                .unwrap()
                .strip_prefix(root)
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/");
            let package_name = package
                .get("name")
                .and_then(toml_edit::Item::as_str)
                .unwrap_or("crate")
                .replace('-', "_");
            let mut targets = vec![];
            let mut add = |path: String, name: String, library: bool, test: bool| {
                if files.contains(&path) && !targets.iter().any(|t: &Target| t.path == path) {
                    targets.push(Target {
                        path,
                        name,
                        library,
                        test,
                    });
                }
            };
            let lib = doc.get("lib");
            if lib.is_some()
                || package.get("autolib").and_then(toml_edit::Item::as_bool) != Some(false)
            {
                let path = lib
                    .and_then(|v| v.get("path"))
                    .and_then(toml_edit::Item::as_str)
                    .unwrap_or("src/lib.rs");
                let name = lib
                    .and_then(|v| v.get("name"))
                    .and_then(toml_edit::Item::as_str)
                    .unwrap_or(&package_name)
                    .replace('-', "_");
                add(joined(&dir, path), name, true, false);
            }
            for (kind, folder, automatic, test) in [
                ("bin", "src/bin", "autobins", false),
                ("test", "tests", "autotests", true),
                ("bench", "benches", "autobenches", false),
                ("example", "examples", "autoexamples", false),
            ] {
                if let Some(tables) = doc.get(kind).and_then(toml_edit::Item::as_array_of_tables) {
                    for table in tables {
                        let name = table
                            .get("name")
                            .and_then(toml_edit::Item::as_str)
                            .unwrap_or(&package_name)
                            .replace('-', "_");
                        if let Some(path) = table.get("path").and_then(toml_edit::Item::as_str) {
                            add(joined(&dir, path), name, false, test);
                        } else {
                            for path in [
                                format!("{folder}/{name}.rs"),
                                format!("{folder}/{name}/main.rs"),
                            ] {
                                add(joined(&dir, &path), name.clone(), false, test);
                            }
                            if kind == "bin" {
                                add(joined(&dir, "src/main.rs"), name, false, test);
                            }
                        }
                    }
                }
                if package.get(automatic).and_then(toml_edit::Item::as_bool) != Some(false) {
                    let prefix = format!("{}/", joined(&dir, folder));
                    for path in files.range(prefix.clone()..) {
                        if !path.starts_with(&prefix) {
                            break;
                        }
                        let rel = &path[prefix.len()..];
                        if (!rel.contains('/') && rel.ends_with(".rs"))
                            || (rel.ends_with("/main.rs") && rel.matches('/').count() == 1)
                        {
                            let name = rel
                                .trim_end_matches("/main.rs")
                                .trim_end_matches(".rs")
                                .replace('-', "_");
                            add(path.clone(), name, false, test);
                        }
                    }
                    if kind == "bin" {
                        add(
                            joined(&dir, "src/main.rs"),
                            package_name.clone(),
                            false,
                            false,
                        );
                    }
                }
            }
            match package.get("build") {
                Some(v) if v.as_bool() == Some(false) => {}
                value => add(
                    joined(
                        &dir,
                        value
                            .and_then(toml_edit::Item::as_str)
                            .unwrap_or("build.rs"),
                    ),
                    "build_script_build".into(),
                    false,
                    false,
                ),
            }
            Ok(targets)
        })
        .collect();
    let mut out = Targets::default();
    for r in results {
        match r {
            Ok(v) => out.entries.extend(v),
            Err(e) => out.errors.push(e),
        }
    }
    out.entries.sort_by(|a, b| a.path.cmp(&b.path));
    out.entries.dedup_by(|a, b| a.path == b.path);
    out
}

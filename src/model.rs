use crate::geometry::{Color, rgb};
use anyhow::{Context, Result};
use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};
use syn::{spanned::Spanned, visit::Visit};

#[derive(Clone)]
pub struct SourceFile {
    pub path: String,
    pub language: &'static str,
    pub text: String,
    pub source_bytes: u64,
    pub escaped_bytes: bool,
    pub is_test: bool,
    pub hidden_lines: BTreeSet<usize>,
    pub lines: Vec<CodeLine>,
}
#[derive(Clone)]
pub struct CodeLine {
    pub number: usize,
    pub spans: Vec<Span>,
}
#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub color: Color,
}
#[derive(Serialize, Clone, Default)]
pub struct ScanStats {
    pub files: usize,
    pub test_files: usize,
    pub lines: usize,
    pub bytes: u64,
    pub scan_ms: f64,
    pub warnings: Vec<String>,
}
pub struct Codebase {
    pub root: PathBuf,
    pub name: String,
    pub files: Vec<SourceFile>,
    pub stats: ScanStats,
    /// Manifest paths are index metadata; they never enter the source visualization.
    pub manifests: Vec<PathBuf>,
    pub cargo: crate::cargo::Targets,
}
pub fn language(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "Rust",
        "py" | "pyi" => "Python",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "c" | "h" => "C",
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" | "cu" | "cuh" => "C++",
        "cs" => "C#",
        "go" => "Go",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "swift" => "Swift",
        "rb" => "Ruby",
        "php" => "PHP",
        "lua" => "Lua",
        "sh" | "bash" | "zsh" | "fish" | "ps1" => "Shell",
        "wgsl" | "glsl" | "vert" | "frag" | "comp" | "hlsl" | "metal" => "Shader",
        "html" | "htm" | "css" | "scss" | "sass" | "svelte" | "vue" => "Web",
        "sql" => "SQL",
        "ex" | "exs" => "Elixir",
        "erl" | "hrl" => "Erlang",
        "hs" | "lhs" => "Haskell",
        "ml" | "mli" => "OCaml",
        "clj" | "cljs" | "cljc" => "Clojure",
        "scala" => "Scala",
        "dart" => "Dart",
        "r" => "R",
        "jl" => "Julia",
        "zig" => "Zig",
        "v" | "sv" | "vhd" | "vhdl" => "HDL",
        "s" | "asm" => "Assembly",
        "proto" => "Protobuf",
        _ => return None,
    })
}
pub fn test_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_ascii_lowercase();
    if p.split('/').any(|p| {
        matches!(
            p,
            "test"
                | "tests"
                | "__tests__"
                | "spec"
                | "specs"
                | "testing"
                | "e2e"
                | "testdata"
                | "fixtures"
                | "__fixtures__"
        )
    }) {
        return true;
    }
    let file = p.rsplit('/').next().unwrap_or(&p);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    matches!(stem, "test" | "tests" | "spec" | "specs")
        || stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.ends_with(".test")
        || stem.ends_with(".spec")
        || stem.ends_with("_spec")
        || path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .rsplit_once('.')
            .is_some_and(|(s, _)| s.ends_with("Test") || s.ends_with("Tests"))
}
fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "node_modules"
            | "target"
            | "build"
            | "dist"
            | "vendor"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".next"
            | ".nuxt"
            | "coverage"
            | ".cache"
            | ".idea"
            | ".vscode"
    )
}
impl Codebase {
    pub fn scan(root: &Path) -> Result<Self> {
        let start = Instant::now();
        let root = root
            .canonicalize()
            .with_context(|| format!("Cannot open {}", root.display()))?;
        anyhow::ensure!(
            root.is_dir(),
            "Choose a directory, not a file: {}",
            root.display()
        );
        let found = Mutex::new((Vec::new(), Vec::new(), Vec::new()));
        WalkBuilder::new(&root)
            .hidden(false)
            .follow_links(false)
            .require_git(false)
            .threads(
                std::thread::available_parallelism()
                    .map_or(16, |n| n.get())
                    .min(32),
            )
            .filter_entry(|e| {
                e.depth() == 0
                    || !e.file_type().is_some_and(|t| t.is_dir())
                    || !skip_dir(&e.file_name().to_string_lossy())
            })
            .build_parallel()
            .run(|| {
                Box::new(|entry| {
                    match entry {
                        Ok(e)
                            if e.file_type().is_some_and(|f| f.is_file())
                                && e.file_name() == "Cargo.toml" =>
                        {
                            found.lock().unwrap().2.push(e.into_path())
                        }
                        Ok(e)
                            if e.file_type().is_some_and(|f| f.is_file())
                                && language(e.path()).is_some() =>
                        {
                            found.lock().unwrap().0.push(e.into_path())
                        }
                        Ok(_) => {}
                        Err(e) => found.lock().unwrap().1.push(e.to_string()),
                    }
                    WalkState::Continue
                })
            });
        let (mut paths, mut warnings, mut manifests) = found.into_inner().unwrap();
        paths.sort();
        manifests.sort();
        let results: Vec<_> = paths
            .par_iter()
            .map(|path| -> Result<SourceFile> {
                let bytes = std::fs::read(path)
                    .with_context(|| format!("Cannot read {}", path.display()))?;
                let source_bytes = bytes.len() as u64;
                let escaped_bytes = std::str::from_utf8(&bytes).is_err();
                let mut text = String::new();
                for chunk in bytes.utf8_chunks() {
                    text.push_str(chunk.valid());
                    for byte in chunk.invalid() {
                        use std::fmt::Write;
                        write!(&mut text, "\\x{byte:02X}").unwrap();
                    }
                }
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let lang = language(path).unwrap();
                let hidden_lines = if lang == "Rust"
                    && !test_path(&rel)
                    && (text.contains("#[cfg")
                        || text.contains("#[test")
                        || text.contains("::test]"))
                {
                    rust_test_lines(&text)
                } else {
                    BTreeSet::new()
                };
                let lines = highlight(&text, lang);
                Ok(SourceFile {
                    is_test: test_path(&rel),
                    path: rel,
                    language: lang,
                    text,
                    source_bytes,
                    escaped_bytes,
                    hidden_lines,
                    lines,
                })
            })
            .collect();
        let mut files = Vec::new();
        for r in results {
            match r {
                Ok(f) => {
                    if f.escaped_bytes {
                        warnings.push(format!(
                            "Non-UTF-8 bytes displayed as \\xNN escapes: {}",
                            f.path
                        ));
                    }
                    files.push(f)
                }
                Err(e) => warnings.push(format!("{e:#}")),
            }
        }
        let cargo = crate::cargo::read(
            &root,
            &files.iter().map(|f| f.path.clone()).collect(),
            &manifests,
        );
        let test_targets: BTreeSet<_> = cargo
            .entries
            .iter()
            .filter(|t| t.test)
            .map(|t| t.path.as_str())
            .collect();
        for f in &mut files {
            if test_targets.contains(f.path.as_str()) {
                f.is_test = true;
            }
        }
        let stats = ScanStats {
            files: files.len(),
            test_files: files.iter().filter(|f| f.is_test).count(),
            lines: files.iter().map(|f| f.lines.len()).sum(),
            bytes: files.iter().map(|f| f.source_bytes).sum(),
            scan_ms: start.elapsed().as_secs_f64() * 1000.,
            warnings,
        };
        let name = root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            root,
            name,
            files,
            stats,
            manifests,
            cargo,
        })
    }
    pub fn chars(&self) -> BTreeSet<char> {
        self.files
            .iter()
            .flat_map(|f| f.text.chars().chain(f.path.chars()))
            .chain(self.name.chars())
            .filter(|c| !c.is_control())
            .collect()
    }
}
fn test_attr(a: &syn::Attribute) -> bool {
    if a.path().is_ident("test") {
        return true;
    }
    if a.path().segments.last().is_some_and(|s| s.ident == "test") {
        return true;
    }
    // Only definitely test-only cfgs. Do not remove production cfg(any(test, feature)).
    if a.path().is_ident("cfg") {
        if let Ok(meta) = a.parse_args::<syn::Meta>() {
            return cfg_requires_test(&meta);
        }
    }
    false
}
fn cfg_requires_test(m: &syn::Meta) -> bool {
    match m {
        syn::Meta::Path(p) => p.is_ident("test"),
        syn::Meta::List(l) if l.path.is_ident("all") => {
            use syn::parse::Parser;
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(l.tokens.clone())
                .is_ok_and(|items| items.iter().any(cfg_requires_test))
        }
        _ => false,
    }
}
fn rust_test_lines(text: &str) -> BTreeSet<usize> {
    struct Finder(BTreeSet<usize>);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_item(&mut self, item: &'ast syn::Item) {
            let attrs = match item {
                syn::Item::Mod(m) => &m.attrs,
                syn::Item::Fn(f) => &f.attrs,
                syn::Item::Use(i) => &i.attrs,
                syn::Item::Const(i) => &i.attrs,
                syn::Item::Static(i) => &i.attrs,
                syn::Item::Struct(i) => &i.attrs,
                syn::Item::Enum(i) => &i.attrs,
                syn::Item::Impl(i) => &i.attrs,
                syn::Item::Type(i) => &i.attrs,
                syn::Item::Macro(i) => &i.attrs,
                _ => {
                    syn::visit::visit_item(self, item);
                    return;
                }
            };
            if attrs.iter().any(test_attr) {
                let first = attrs
                    .first()
                    .map_or(item.span().start().line, |a| a.span().start().line);
                self.0.extend(first..=item.span().end().line);
            } else {
                syn::visit::visit_item(self, item);
            }
        }
    }
    let mut f = Finder(BTreeSet::new());
    if let Ok(ast) = syn::parse_file(text) {
        f.visit_file(&ast);
    }
    f.0
}
pub fn language_color(lang: &str) -> Color {
    rgb(match lang {
        "Rust" => 0xe5ad87,
        "Python" => 0xe6cb75,
        "TypeScript" => 0x80b7ee,
        "JavaScript" => 0xe8d48a,
        "Shader" => 0xbea1ee,
        "C++" | "C" => 0x8cacfa,
        "Web" => 0xe79fad,
        "Go" => 0x75cfd3,
        "Shell" => 0x9dccad,
        _ => 0x9cb8c9,
    })
}
fn highlight(text: &str, lang: &str) -> Vec<CodeLine> {
    let mut block = 0u32;
    let mut multiline: Option<char> = None;
    let hash = matches!(lang, "Python" | "Ruby" | "Shell" | "R" | "Julia");
    text.lines()
        .enumerate()
        .map(|(n, line)| {
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            let mut spans: Vec<Span> = Vec::new();
            while i < chars.len() {
                let start = i;
                let c = chars[i];
                let mut color = rgb(0xdce4ed);
                if block > 0 {
                    color = rgb(0x99aaad);
                    while i < chars.len() {
                        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '/' {
                            i += 2;
                            block -= 1;
                            break;
                        }
                        i += 1;
                    }
                } else if let Some(q) = multiline {
                    color = rgb(0xb9d9a8);
                    while i < chars.len() {
                        if chars[i] == q {
                            if q == '`' {
                                i += 1;
                                multiline = None;
                                break;
                            }
                            if chars.get(i..i + 3) == Some(&[q, q, q]) {
                                i += 3;
                                multiline = None;
                                break;
                            }
                        }
                        i += 1;
                    }
                } else if (hash && c == '#')
                    || (c == '/' && chars.get(i + 1) == Some(&'/'))
                    || (matches!(lang, "SQL" | "Haskell" | "Lua")
                        && c == '-'
                        && chars.get(i + 1) == Some(&'-'))
                {
                    color = rgb(0x99aaad);
                    i = chars.len();
                } else if c == '/' && chars.get(i + 1) == Some(&'*') {
                    color = rgb(0x99aaad);
                    block = 1;
                    i += 2;
                    while i < chars.len() {
                        if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                            i += 2;
                            block = 0;
                            break;
                        }
                        i += 1;
                    }
                } else if c == '"'
                    || c == '`'
                    || (c == '\''
                        && (lang != "Rust"
                            || chars.get(i + 2) == Some(&'\'')
                            || chars.get(i + 1) == Some(&'\\')))
                {
                    color = rgb(0xb9d9a8);
                    let triple = chars.get(i..i + 3) == Some(&[c, c, c]);
                    i += if triple { 3 } else { 1 };
                    if triple || c == '`' {
                        multiline = Some(c);
                    }
                    while i < chars.len() {
                        if chars[i] == '\\' {
                            i = (i + 2).min(chars.len());
                            continue;
                        }
                        if chars[i] == c {
                            if !triple {
                                i += 1;
                                multiline = None;
                                break;
                            }
                            if chars.get(i..i + 3) == Some(&[c, c, c]) {
                                i += 3;
                                multiline = None;
                                break;
                            }
                        }
                        i += 1;
                    }
                } else if c.is_alphabetic() || c == '_' {
                    i += 1;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    if matches!(
                        word.as_str(),
                        "fn" | "pub"
                            | "use"
                            | "mod"
                            | "struct"
                            | "enum"
                            | "impl"
                            | "trait"
                            | "let"
                            | "mut"
                            | "const"
                            | "static"
                            | "async"
                            | "await"
                            | "move"
                            | "self"
                            | "Self"
                            | "super"
                            | "crate"
                            | "where"
                            | "dyn"
                            | "type"
                            | "as"
                            | "in"
                            | "if"
                            | "else"
                            | "match"
                            | "return"
                            | "for"
                            | "while"
                            | "loop"
                            | "break"
                            | "continue"
                            | "unsafe"
                            | "extern"
                            | "def"
                            | "class"
                            | "import"
                            | "from"
                            | "with"
                            | "try"
                            | "except"
                            | "raise"
                            | "pass"
                            | "lambda"
                            | "yield"
                            | "function"
                            | "export"
                            | "default"
                            | "new"
                            | "this"
                            | "var"
                            | "interface"
                            | "extends"
                            | "private"
                            | "public"
                            | "protected"
                            | "void"
                            | "package"
                            | "func"
                            | "defer"
                            | "switch"
                            | "case"
                            | "select"
                            | "go"
                            | "throw"
                            | "catch"
                            | "finally"
                            | "val"
                    ) {
                        color = rgb(0xdec1f2);
                    } else if matches!(
                        word.as_str(),
                        "true" | "false" | "null" | "None" | "Some" | "True" | "False" | "nil"
                    ) {
                        color = rgb(0xf1c998);
                    } else if c.is_uppercase() {
                        color = rgb(0xefdda6);
                    } else if chars.get(i) == Some(&'(') || chars.get(i) == Some(&'!') {
                        color = rgb(0x9fdef0);
                    }
                } else if c.is_ascii_digit() {
                    color = rgb(0xf1c998);
                    i += 1;
                    while i < chars.len()
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_')
                    {
                        i += 1;
                    }
                } else {
                    i += 1;
                }
                let value: String = chars[start..i].iter().collect();
                if let Some(last) = spans.last_mut().filter(|s| s.color == color) {
                    last.text.push_str(&value);
                } else {
                    spans.push(Span { text: value, color });
                }
            }
            CodeLine {
                number: n + 1,
                spans,
            }
        })
        .collect()
}

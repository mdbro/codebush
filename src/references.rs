//! Auditable static file references. No symbol-name or comment co-occurrence edges.
use crate::{
    model::Codebase,
    projection::{self, PLANES, Vector},
};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};
use syn::{spanned::Spanned, visit::Visit};

#[derive(Clone, Debug, Serialize)]
pub struct Reference {
    pub from: usize,
    pub to: usize,
    pub line: usize,
    pub spelling: String,
    pub kind: String,
    pub test_only: bool,
    pub plane: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Unresolved {
    pub file: usize,
    pub line: usize,
    pub spelling: String,
    pub reason: String,
    pub kind: String,
    pub inline_depth: usize,
}
#[derive(Serialize)]
pub struct Graph {
    pub edges: Vec<Reference>,
    pub unresolved: Vec<Unresolved>,
    pub positions: Vec<Vector>,
    pub masks: [Vec<[u32; 3]>; 2],
    pub plane_edges: [Vec<Vec<usize>>; 2],
    pub unsupported_languages: BTreeMap<String, usize>,
    pub parse_failures: Vec<String>,
    pub manifest_failures: Vec<String>,
}
struct Index<'a> {
    base: &'a Codebase,
    paths: HashMap<String, usize>,
    suffixes: HashMap<String, Vec<usize>>,
    roots: Vec<(String, String)>,
    root_files: BTreeSet<usize>,
    scope_roots: Vec<String>,
}
fn normal(p: &str) -> String {
    let mut v = Vec::new();
    for s in p.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                v.pop();
            }
            _ => v.push(s),
        }
    }
    v.join("/")
}
fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |x| x.0)
}
fn join(a: &str, b: &str) -> String {
    normal(&format!("{a}/{b}"))
}
impl<'a> Index<'a> {
    fn new(base: &'a Codebase) -> Self {
        let paths: HashMap<String, usize> = base
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.clone(), i))
            .collect();
        let mut suffixes: HashMap<String, Vec<usize>> = HashMap::new();
        let mut roots = BTreeSet::new();
        let mut root_files = BTreeSet::new();
        for (i, f) in base.files.iter().enumerate() {
            let parts: Vec<_> = f.path.split('/').collect();
            for k in 0..parts.len() {
                suffixes.entry(parts[k..].join("/")).or_default().push(i);
            }
            if f.language == "Rust" && matches!(parts.last(), Some(&"lib.rs") | Some(&"main.rs")) {
                root_files.insert(i);
                let dir = parent(&f.path).to_string();
                let package = if dir.ends_with("/src") {
                    parent(&dir)
                } else if dir == "src" {
                    ""
                } else {
                    &dir
                };
                let name = package
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&base.name)
                    .replace('-', "_");
                roots.insert((dir, name));
            }
        }
        let mut scopes: BTreeSet<_> = roots.iter().map(|(dir, _)| dir.clone()).collect();
        for target in &base.cargo.entries {
            if let Some(&id) = paths.get(&target.path) {
                root_files.insert(id);
                let dir = parent(&target.path).to_owned();
                scopes.insert(dir.clone());
                if target.library {
                    roots.retain(|(d, _)| d != &dir);
                    roots.insert((dir, target.name.clone()));
                }
            }
        }
        let mut scope_roots: Vec<_> = scopes.into_iter().collect();
        scope_roots.sort_by_key(|d| std::cmp::Reverse(d.len()));
        let mut roots: Vec<_> = roots.into_iter().collect();
        roots.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
        Self {
            base,
            paths,
            suffixes,
            roots,
            root_files,
            scope_roots,
        }
    }
    fn root(&self, file: usize) -> String {
        let p = &self.base.files[file].path;
        if self.root_files.contains(&file) {
            return parent(p).to_owned();
        }
        self.scope_roots
            .iter()
            .find(|d| d.is_empty() || p.starts_with(&format!("{d}/")))
            .cloned()
            .unwrap_or_else(|| parent(p).to_string())
    }
    fn exact(&self, p: &str) -> Option<usize> {
        self.paths.get(&normal(p)).copied()
    }
    fn unique(&self, p: &str) -> Option<usize> {
        self.suffixes.get(p).filter(|x| x.len() == 1).map(|x| x[0])
    }
    fn file_path(&self, from: usize, p: &str, exts: &[&str]) -> Option<usize> {
        let dir = parent(&self.base.files[from].path);
        let p = p.replace('\\', "/");
        for stem in [join(dir, &p), normal(&p)] {
            if let Some(id) = self.exact(&stem) {
                return Some(id);
            }
            for ext in exts {
                if let Some(id) = self.exact(&format!("{stem}{ext}")) {
                    return Some(id);
                }
            }
        }
        if let Some(id) = self.unique(&p) {
            return Some(id);
        }
        for ext in exts {
            if let Some(id) = self.unique(&format!("{p}{ext}")) {
                return Some(id);
            }
        }
        None
    }
    fn rust_path(&self, file: usize, segments: &[String], inline: &[String]) -> Option<usize> {
        if segments.is_empty() {
            return None;
        }
        let path = &self.base.files[file].path;
        let mut scope = parent(path).to_string();
        if !self.root_files.contains(&file)
            && !matches!(
                Path::new(path).file_name()?.to_str()?,
                "lib.rs" | "main.rs" | "mod.rs"
            )
        {
            scope = path.trim_end_matches(".rs").into();
        }
        for s in inline {
            scope = join(&scope, s);
        }
        let mut rest = segments;
        let first = rest[0].as_str();
        if first == "crate" {
            scope = self.root(file);
            rest = &rest[1..];
        } else if first == "self" {
            rest = &rest[1..];
        } else if first == "super" {
            while rest.first().is_some_and(|x| x == "super") {
                scope = parent(&scope).to_string();
                rest = &rest[1..];
            }
        } else {
            let roots: Vec<_> = self.roots.iter().filter(|(_, n)| n == first).collect();
            if roots.len() == 1 {
                scope = roots[0].0.clone();
                rest = &rest[1..];
            }
        }
        for len in (1..=rest.len()).rev() {
            let s = join(&scope, &rest[..len].join("/"));
            for suffix in [".rs", "/mod.rs", "/lib.rs"] {
                if let Some(id) = self.exact(&format!("{s}{suffix}")) {
                    return Some(id);
                }
            }
        }
        if rest.is_empty() && segments[0] == "self" {
            return Some(file);
        }
        // A parent/root file cannot be inferred from a shared src directory.
        // Resolve it from external-module declarations after extraction.

        None
    }
}
struct Extract<'a, 'b> {
    idx: &'a Index<'b>,
    file: usize,
    inline: Vec<String>,
    edges: Vec<Reference>,
    unresolved: Vec<Unresolved>,
}
impl Extract<'_, '_> {
    fn add(&mut self, to: Option<usize>, line: usize, spelling: String, kind: &str) {
        if let Some(to) = to {
            if to != self.file {
                let f = &self.idx.base.files[self.file];
                let other = &self.idx.base.files[to];
                let key = format!("{}\0{}", f.path, other.path);
                self.edges.push(Reference {
                    from: self.file,
                    to,
                    line,
                    spelling,
                    kind: kind.into(),
                    test_only: f.is_test || other.is_test || f.hidden_lines.contains(&line),
                    plane: (projection::hash(&key) % PLANES as u64) as usize,
                });
            }
        } else {
            self.unresolved.push(Unresolved {
                file: self.file,
                line,
                spelling,
                reason: "External, dynamic, ambiguous, or not resolvable in indexed source".into(),
                kind: kind.into(),
                inline_depth: self.inline.len(),
            });
        }
    }
    fn use_tree(&mut self, t: &syn::UseTree, prefix: Vec<String>, line: usize) {
        match t {
            syn::UseTree::Path(p) => {
                let mut s = prefix;
                s.push(p.ident.to_string());
                self.use_tree(&p.tree, s, line);
            }
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.use_tree(t, prefix.clone(), line)
                }
            }
            _ => {
                let mut s = prefix;
                match t {
                    syn::UseTree::Name(n) => {
                        if n.ident != "self" {
                            s.push(n.ident.to_string());
                        }
                    }
                    syn::UseTree::Rename(r) => s.push(r.ident.to_string()),
                    _ => {}
                }
                let to = self.idx.rust_path(self.file, &s, &self.inline);
                self.add(to, line, s.join("::"), "Rust use");
            }
        }
    }
}
impl<'ast> Visit<'ast> for Extract<'_, '_> {
    fn visit_item_use(&mut self, n: &'ast syn::ItemUse) {
        self.use_tree(&n.tree, vec![], n.span().start().line);
    }
    fn visit_item_mod(&mut self, n: &'ast syn::ItemMod) {
        if let Some((_, items)) = &n.content {
            self.inline.push(n.ident.to_string());
            for item in items {
                self.visit_item(item);
            }
            self.inline.pop();
        } else {
            let explicit = n.attrs.iter().find_map(|a| {
                if a.path().is_ident("path") {
                    if let syn::Meta::NameValue(v) = &a.meta {
                        if let syn::Expr::Lit(e) = &v.value {
                            if let syn::Lit::Str(s) = &e.lit {
                                return Some(s.value());
                            }
                        }
                    }
                }
                None
            });
            let name = n.ident.to_string();
            let to = if let Some(p) = &explicit {
                self.idx.file_path(self.file, p, &[])
            } else {
                self.idx
                    .rust_path(self.file, &["self".into(), name.clone()], &self.inline)
            };
            self.add(
                to,
                n.span().start().line,
                explicit.unwrap_or(name),
                "Rust module",
            );
        }
    }
    fn visit_path(&mut self, n: &'ast syn::Path) {
        if n.segments.len() > 1 {
            let s: Vec<_> = n.segments.iter().map(|s| s.ident.to_string()).collect();
            // Only resolve qualified paths against actual files; unresolved ordinary symbols
            // are not counted as declared file references.
            if let Some(to) = self.idx.rust_path(self.file, &s, &self.inline) {
                self.add(
                    Some(to),
                    n.span().start().line,
                    s.join("::"),
                    "Rust qualified path",
                );
            }
        }
        syn::visit::visit_path(self, n);
    }
    fn visit_macro(&mut self, n: &'ast syn::Macro) {
        if n.path.is_ident("include")
            || n.path.is_ident("include_str")
            || n.path.is_ident("include_bytes")
            || n.path
                .segments
                .last()
                .is_some_and(|s| s.ident == "include_wgsl" || s.ident == "include_spirv")
        {
            if let Ok(s) = syn::parse2::<syn::LitStr>(n.tokens.clone()) {
                let value = s.value();
                let to = self.idx.file_path(self.file, &value, &[]);
                self.add(to, n.span().start().line, value, "Rust include");
            }
        }
    }
}
#[derive(Debug)]
struct Token {
    s: String,
    line: usize,
    string: bool,
}
fn tokens(text: &str, language: &str) -> Vec<Token> {
    let chars: Vec<_> = text.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if (c == '#' && matches!(language, "Python" | "Shell" | "Ruby" | "PHP"))
            || (c == '-' && chars.get(i + 1) == Some(&'-') && language == "Lua")
        {
            if language == "Lua" && chars.get(i + 2) == Some(&'[') && chars.get(i + 3) == Some(&'[')
            {
                i += 4;
                while i + 1 < chars.len() && !(chars[i] == ']' && chars[i + 1] == ']') {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i = (i + 2).min(chars.len());
            } else {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            continue;
        }
        if language == "Lua" && c == '[' && chars.get(i + 1) == Some(&'[') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == ']' && chars[i + 1] == ']') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        let start = line;
        let string = c == '\'' || c == '"' || c == '`';
        let mut s = String::new();
        if string {
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    s.push(chars[i]);
                    i += 1;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            i = (i + 1).min(chars.len());
        } else if c.is_alphanumeric() || c == '_' || c == '$' {
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                s.push(chars[i]);
                i += 1;
            }
        } else {
            s.push(c);
            i += 1;
        }
        out.push(Token {
            s,
            line: start,
            string,
        });
    }
    out
}
fn generic(x: &mut Extract<'_, '_>) {
    let f = &x.idx.base.files[x.file];
    let ts = tokens(&f.text, f.language);
    let lang = f.language;
    for i in 0..ts.len() {
        let t = &ts[i];
        if t.string {
            continue;
        }
        let next = ts.get(i + 1);
        let line = t.line;
        if t.s == "include" && i > 0 && ts[i - 1].s == "#" {
            let mut value = String::new();
            if let Some(n) = next {
                if n.string {
                    value = n.s.clone();
                } else if n.s == "<" {
                    for v in &ts[i + 2..] {
                        if v.s == ">" || v.line != line {
                            break;
                        }
                        value.push_str(&v.s);
                    }
                }
            }
            if !value.is_empty() {
                let to = x.idx.file_path(x.file, &value, &[]);
                x.add(to, line, value, "include");
            }
            continue;
        }
        if matches!(lang, "JavaScript" | "TypeScript" | "Web")
            && matches!(t.s.as_str(), "from" | "import" | "require")
        {
            if i > 0 && matches!(ts[i - 1].s.as_str(), "." | "?.") {
                continue;
            }
            if t.s == "from"
                && !ts[..i]
                    .iter()
                    .rev()
                    .take_while(|n| n.s != ";")
                    .any(|n| !n.string && matches!(n.s.as_str(), "import" | "export"))
            {
                continue;
            }
            let n = if next.is_some_and(|n| n.s == "(") {
                ts.get(i + 2)
            } else {
                next
            };
            if let Some(n) = n.filter(|n| n.string) {
                let to = x.idx.file_path(
                    x.file,
                    &n.s,
                    &[
                        ".ts",
                        ".tsx",
                        ".js",
                        ".jsx",
                        ".mjs",
                        "/index.ts",
                        "/index.tsx",
                        "/index.js",
                    ],
                );
                x.add(to, line, n.s.clone(), "JS import");
            }
            continue;
        }
        if lang == "Python" && matches!(t.s.as_str(), "from" | "import") {
            // Ignore Python comment tails, including a quoted fake import.
            if ts[..i]
                .iter()
                .rev()
                .take_while(|v| v.line == line)
                .any(|v| v.s == "#")
            {
                continue;
            }
            let mut name = String::new();
            let mut end = i + 1;
            while end < ts.len()
                && ts[end].line == line
                && (ts[end].s == "." || ts[end].s.chars().all(|c| c.is_alphanumeric() || c == '_'))
            {
                if matches!(ts[end].s.as_str(), "import" | "as") {
                    break;
                }
                name.push_str(&ts[end].s);
                end += 1;
            }
            if !name.is_empty() {
                let dots = name.len() - name.trim_start_matches('.').len();
                let part = name.trim_start_matches('.').replace('.', "/");
                let mut p = parent(&f.path).to_string();
                for _ in 1..dots {
                    p = parent(&p).into();
                }
                let path = if dots > 0 { join(&p, &part) } else { part };
                let to = x.idx.file_path(x.file, &path, &[".py", "/__init__.py"]);
                x.add(to, line, name.clone(), "Python import");
                if t.s == "from" && ts.get(end).is_some_and(|v| v.s == "import") {
                    for n in ts[end + 1..].iter().take_while(|v| v.line == line) {
                        if n.s.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            let full = join(&path, &n.s);
                            if let Some(to) =
                                x.idx.file_path(x.file, &full, &[".py", "/__init__.py"])
                            {
                                x.add(Some(to), line, format!("{name}.{}", n.s), "Python from");
                            }
                        }
                    }
                }
            }
            continue;
        }
        if lang == "Go" && t.s == "import" {
            let group = next.is_some_and(|t| t.s == "(");
            for n in ts[i + 1..]
                .iter()
                .take_while(|n| if group { n.s != ")" } else { n.line == line })
            {
                if n.string {
                    let suffix = n.s.rsplit('/').next().unwrap_or(&n.s);
                    let candidates: Vec<_> = x
                        .idx
                        .base
                        .files
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.language == "Go" && parent(&f.path).ends_with(suffix))
                        .map(|(i, _)| i)
                        .collect();
                    let dirs: BTreeSet<_> = candidates
                        .iter()
                        .map(|&j| parent(&x.idx.base.files[j].path))
                        .collect();
                    if dirs.len() == 1 {
                        for to in candidates {
                            x.add(Some(to), line, n.s.clone(), "Go package");
                        }
                    } else {
                        x.add(None, line, n.s.clone(), "Go package");
                    }
                }
            }
            continue;
        }
        if matches!(lang, "Java" | "Kotlin") && t.s == "import" {
            let value = ts[i + 1..]
                .iter()
                .take_while(|n| n.line == line && n.s != ";")
                .map(|n| n.s.as_str())
                .collect::<String>()
                .replace('.', "/");
            let to = x.idx.file_path(x.file, &value, &[".java", ".kt"]);
            x.add(to, line, value, "JVM import");
        }
        if matches!(lang, "Shell" | "Ruby" | "PHP" | "Lua")
            && matches!(
                t.s.as_str(),
                "source" | "require" | "require_relative" | "include" | "include_once" | "dofile"
            )
        {
            let n = if next.is_some_and(|n| n.s == "(") {
                ts.get(i + 2)
            } else {
                next
            };
            if let Some(n) = n.filter(|n| n.string) {
                let to = x
                    .idx
                    .file_path(x.file, &n.s, &[".rb", ".lua", ".php", ".sh"]);
                x.add(to, line, n.s.clone(), "source import");
            }
        }
    }
}

// Rust's compiler sources can use syntax newer than syn's AST. Token groups still
// preserve comments/strings, source spans, and the boundaries of valid import items.
fn rust_items(x: &mut Extract<'_, '_>, stream: proc_macro2::TokenStream) {
    use proc_macro2::{Delimiter, TokenTree};
    let ts: Vec<_> = stream.into_iter().collect();
    let mut i = 0;
    while i < ts.len() {
        if let TokenTree::Ident(word) = &ts[i] {
            if word == "use" {
                let mut end = i + 1;
                while end < ts.len() && !matches!(&ts[end],TokenTree::Punct(p) if p.as_char()==';')
                {
                    end += 1;
                }
                if end < ts.len() {
                    let item: proc_macro2::TokenStream = ts[i..=end].iter().cloned().collect();
                    if let Ok(item) = syn::parse2::<syn::ItemUse>(item) {
                        x.visit_item_use(&item);
                        i = end + 1;
                        continue;
                    }
                }
            }
            if word == "mod" {
                if let Some(TokenTree::Ident(name)) = ts.get(i + 1) {
                    if let Some(next) = ts.get(i + 2) {
                        match next {
                            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                                x.inline.push(name.to_string());
                                rust_items(x, g.stream());
                                x.inline.pop();
                                i += 3;
                                continue;
                            }
                            TokenTree::Punct(p) if p.as_char() == ';' => {
                                let spelling = name.to_string();
                                let to = x.idx.rust_path(
                                    x.file,
                                    &["self".into(), spelling.clone()],
                                    &x.inline,
                                );
                                x.add(to, word.span().start().line, spelling, "Rust module");
                                i += 3;
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if let TokenTree::Group(g) = &ts[i] {
            if i == 0 || !matches!(&ts[i-1],TokenTree::Punct(p) if p.as_char()=='!') {
                rust_items(x, g.stream());
            }
        }
        i += 1;
    }
}
fn resolve_parent_imports(
    base: &Codebase,
    edges: &mut Vec<Reference>,
    unresolved: &mut Vec<Unresolved>,
) {
    let mut parents: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    for e in edges.iter().filter(|e| e.kind == "Rust module") {
        parents.entry(e.to).or_default().insert(e.from);
    }
    let mut retained = Vec::new();
    let mut declarations: HashMap<usize, BTreeSet<String>> = HashMap::new();
    for u in unresolved.drain(..) {
        if u.kind != "Rust use" || base.files[u.file].language != "Rust" {
            retained.push(u);
            continue;
        }
        let s: Vec<_> = u.spelling.split("::").collect();
        let supers = s.iter().take_while(|&&s| s == "super").count();
        let root_import = s.first() == Some(&"crate");
        if supers == 0 && !root_import {
            retained.push(u);
            continue;
        }
        let remaining = &s[if root_import { 1 } else { supers }..];
        let mut owners = BTreeSet::from([u.file]);
        if root_import {
            let mut pending = vec![u.file];
            let mut seen = BTreeSet::new();
            owners.clear();
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                if let Some(parent) = parents.get(&id) {
                    pending.extend(parent.iter().copied());
                } else {
                    owners.insert(id);
                }
            }
        } else {
            for _ in 0..supers.saturating_sub(u.inline_depth) {
                owners = owners
                    .iter()
                    .flat_map(|i| parents.get(i).into_iter().flatten().copied())
                    .collect();
            }
        }
        let mut found = false;
        for to in owners {
            // Wildcard parent imports are certain. For a named item require its declaration.
            let declared = remaining.is_empty()
                || declarations
                    .entry(to)
                    .or_insert_with(|| declarations_in(&base.files[to].text))
                    .contains(remaining[0]);
            if !declared {
                continue;
            }
            found = true;
            if to == u.file {
                continue;
            }
            let f = &base.files[u.file];
            let target = &base.files[to];
            let key = format!("{}\0{}", f.path, target.path);
            edges.push(Reference {
                from: u.file,
                to,
                line: u.line,
                spelling: u.spelling.clone(),
                kind: if root_import {
                    "Rust crate import"
                } else {
                    "Rust parent import"
                }
                .into(),
                test_only: f.is_test || target.is_test || f.hidden_lines.contains(&u.line),
                plane: (projection::hash(&key) % PLANES as u64) as usize,
            });
        }
        if !found {
            retained.push(u);
        }
    }
    *unresolved = retained;
}
fn declarations_in(text: &str) -> BTreeSet<String> {
    use proc_macro2::TokenTree;
    let Ok(stream) = text.parse::<proc_macro2::TokenStream>() else {
        return BTreeSet::new();
    };
    let ts: Vec<_> = stream.into_iter().collect();
    let mut names = BTreeSet::new();
    for w in ts.windows(2) {
        if let (TokenTree::Ident(kind), TokenTree::Ident(id)) = (&w[0], &w[1]) {
            if matches!(
                kind.to_string().as_str(),
                "fn" | "struct" | "enum" | "type" | "trait" | "const" | "static" | "mod" | "union"
            ) {
                names.insert(id.to_string());
            }
        }
    }
    names
}
fn mark_test_context(base: &Codebase, edges: &mut [Reference]) {
    let mut parents: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
    for e in edges.iter().filter(|e| e.kind == "Rust module") {
        parents.entry(e.to).or_default().push((e.from, e.test_only));
    }
    let tests: Vec<_> = base
        .files
        .iter()
        .enumerate()
        .map(|(file, f)| {
            if f.is_test {
                return true;
            }
            if f.language != "Rust" {
                return false;
            }
            let mut pending = vec![(file, false)];
            let mut seen = BTreeSet::new();
            let mut reached = false;
            while let Some((id, conditional)) = pending.pop() {
                if !seen.insert((id, conditional)) {
                    continue;
                }
                if let Some(ps) = parents.get(&id) {
                    pending.extend(ps.iter().map(|&(p, t)| (p, conditional || t)));
                } else {
                    reached = true;
                    if !conditional && !base.files[id].is_test {
                        return false;
                    }
                }
            }
            reached
        })
        .collect();
    for e in edges {
        e.test_only |= tests[e.from] || tests[e.to];
    }
}
/// Relax annotation clearances ONCE in the fixed 12D embedding. No per-view anchor motion.
fn relax_positions(positions: &mut [Vector], masks: &[[u32; 3]]) {
    for _ in 0..32 {
        let mut moved = 0;
        for (plane, &[a, b]) in projection::PAIRS.iter().enumerate() {
            let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for id in 0..positions.len() {
                if masks[id][plane / 32] & (1 << (plane % 32)) == 0 {
                    continue;
                }
                let p = positions[id];
                let key = ((p[a] / 920.).floor() as i32, (p[b] / 920.).floor() as i32);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        if let Some(others) = cells.get(&(key.0 + dx, key.1 + dy)) {
                            for &other in others {
                                let delta = [
                                    positions[id][a] - positions[other][a],
                                    positions[id][b] - positions[other][b],
                                ];
                                if delta[0].abs() < 850. && delta[1].abs() < 850. {
                                    let k = if delta[0].abs() > delta[1].abs() {
                                        0
                                    } else {
                                        1
                                    };
                                    let axis = if k == 0 { a } else { b };
                                    let step = (850. - delta[k].abs() + 0.1)
                                        * 0.5
                                        * if delta[k] >= 0. { 1. } else { -1. };
                                    positions[id][axis] += step;
                                    positions[other][axis] -= step;
                                    moved += 1;
                                }
                            }
                        }
                    }
                }
                let p = positions[id];
                cells
                    .entry(((p[a] / 920.).floor() as i32, (p[b] / 920.).floor() as i32))
                    .or_default()
                    .push(id);
            }
        }
        if moved == 0 {
            break;
        }
    }
}
impl Graph {
    pub fn build(base: &Codebase) -> Self {
        let idx = Index::new(base);
        let results: Vec<_> = base
            .files
            .par_iter()
            .enumerate()
            .map(|(file, f)| {
                let mut x = Extract {
                    idx: &idx,
                    file,
                    inline: vec![],
                    edges: vec![],
                    unresolved: vec![],
                };
                let mut failed = None;
                if f.language == "Rust" {
                    match syn::parse_file(&f.text) {
                        Ok(ast) => x.visit_file(&ast),
                        Err(_) => {
                            failed = Some(f.path.clone());
                            if let Ok(stream) = f.text.parse::<proc_macro2::TokenStream>() {
                                rust_items(&mut x, stream);
                            }
                        }
                    }
                } else {
                    generic(&mut x);
                }
                (x.edges, x.unresolved, failed)
            })
            .collect();
        let mut edges = Vec::new();
        let mut unresolved = Vec::new();
        let mut parse_failures = Vec::new();
        for (e, u, p) in results {
            edges.extend(e);
            unresolved.extend(u);
            parse_failures.extend(p);
        }
        resolve_parent_imports(base, &mut edges, &mut unresolved);
        mark_test_context(base, &mut edges);
        edges.sort_by(|a, b| {
            (a.from, a.to, a.test_only, a.line).cmp(&(b.from, b.to, b.test_only, b.line))
        });
        edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.test_only == b.test_only);
        unresolved
            .sort_by(|a, b| (a.file, a.line, &a.spelling).cmp(&(b.file, b.line, &b.spelling)));
        unresolved
            .dedup_by(|a, b| a.file == b.file && a.line == b.line && a.spelling == b.spelling);
        let mut masks = [
            vec![[0; 3]; base.files.len()],
            vec![[0; 3]; base.files.len()],
        ];
        let mut plane_edges = [vec![vec![]; PLANES], vec![vec![]; PLANES]];
        for (i, e) in edges.iter().enumerate() {
            for mode in 0..2 {
                if mode == 0 && e.test_only {
                    continue;
                }
                plane_edges[mode][e.plane].push(i);
                for file in [e.from, e.to] {
                    masks[mode][file][e.plane / 32] |= 1 << (e.plane % 32);
                }
            }
        }
        let largest = plane_edges[1]
            .iter()
            .map(|ids| {
                ids.iter()
                    .flat_map(|&i| [edges[i].from, edges[i].to])
                    .collect::<BTreeSet<_>>()
                    .len()
            })
            .max()
            .unwrap_or(1);
        let spread = (largest as f64).sqrt().max(2.) * 1100.;
        let mut positions: Vec<Vector> = base
            .files
            .iter()
            .map(|f| std::array::from_fn(|d| projection::coordinate(&f.path, d) * spread))
            .collect();
        relax_positions(&mut positions, &masks[1]);
        let mut unsupported_languages = BTreeMap::new();
        for f in &base.files {
            if !matches!(
                f.language,
                "Rust"
                    | "C"
                    | "C++"
                    | "Shader"
                    | "JavaScript"
                    | "TypeScript"
                    | "Web"
                    | "Python"
                    | "Go"
                    | "Java"
                    | "Kotlin"
                    | "Shell"
                    | "Ruby"
                    | "PHP"
                    | "Lua"
            ) {
                *unsupported_languages.entry(f.language.into()).or_default() += 1;
            }
        }
        Self {
            edges,
            unresolved,
            positions,
            masks,
            plane_edges,
            unsupported_languages,
            parse_failures,
            manifest_failures: base.cargo.errors.clone(),
        }
    }
    pub fn active(&self, mode: usize, weights: &[f32; PLANES]) -> Vec<(usize, f32)> {
        let mut visible = [0u32; 3];
        for (p, &w) in weights.iter().enumerate() {
            if w > 1e-6 {
                visible[p / 32] |= 1 << (p % 32);
            }
        }
        self.masks[mode]
            .iter()
            .enumerate()
            .filter_map(|(id, m)| {
                let mut a = 0f32;
                for word in 0..3 {
                    let mut bits = m[word] & visible[word];
                    while bits != 0 {
                        a = a.max(weights[word * 32 + bits.trailing_zeros() as usize]);
                        bits &= bits - 1;
                    }
                }
                (a > 1e-6).then_some((id, a))
            })
            .collect()
    }
}

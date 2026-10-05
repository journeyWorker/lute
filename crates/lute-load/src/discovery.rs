//! Source discovery and parse-only project helpers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Span, TextIndex};
use rayon::prelude::*;

use crate::cache::InputCache;

pub fn find_lute_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() { stack.push(path); }
            else if path.extension().and_then(|e| e.to_str()) == Some("lute") { out.push(path); }
        }
    }
    out.sort();
    let mut deduped = Vec::with_capacity(out.len());
    let mut seen = BTreeSet::new();
    for path in out {
        let canon = std::fs::canonicalize(&path)?;
        if seen.insert(canon) {
            deduped.push(path);
        }
    }
    Ok(deduped)
}

pub fn project_root_for(file: &Path, walk_root: &Path) -> PathBuf {
    let mut dir = file.parent().unwrap_or(walk_root);
    loop {
        if dir.join("lute.project.yaml").is_file() { return dir.to_path_buf(); }
        if dir == walk_root { return walk_root.to_path_buf(); }
        dir = match dir.parent() { Some(parent) => parent, None => return walk_root.to_path_buf() };
    }
}

pub fn parse_project_docs(dir: &Path, files: &[PathBuf]) -> Vec<std::io::Result<(lute_syntax::ast::Document, Vec<Diagnostic>)>> {
    let cache = InputCache::default();
    files.par_iter().map(|file| {
        let text = std::fs::read_to_string(file)?;
        let root = project_root_for(file, dir);
        let (built, (mut doc, diags)) = crate::input::assemble_input(&cache, file, text, None, Some(&root), None);
        let _ = lute_check::desugar_document(&mut doc, &built.input);
        Ok((doc, diags))
    }).collect()
}

pub fn normalize_span_from_text(text: &str, span: Span) -> Span {
    let len = text.len();
    let mut start = span.byte_start.min(len);
    let mut end = span.byte_end.min(len).max(start);
    while start > 0 && !text.is_char_boundary(start) { start -= 1; }
    while end < len && !text.is_char_boundary(end) { end += 1; }
    let idx = TextIndex::new(text);
    Span::from_bytes(&idx, start, end)
}

pub fn nearest_manifest_dir(file: &Path) -> Option<PathBuf> {
    let abs = std::fs::canonicalize(file).ok()?;
    let start = if abs.is_dir() { abs.as_path() } else { abs.parent()? };
    start.ancestors().find(|d| d.join("lute.project.yaml").is_file()).map(Path::to_path_buf)
}

pub fn discover_project(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    if project.is_some() { return None; }
    let dir = nearest_manifest_dir(file)?;
    eprintln!("lute: note: using project {} (nearest lute.project.yaml); pass --project to choose another", dir.display());
    Some(dir)
}

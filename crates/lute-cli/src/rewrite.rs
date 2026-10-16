//! `lute tag` / `lute fix` — the in-place source rewrites, over one `.lute`
//! file or every `.lute` file under a directory (dsl 0.22.0 §13).
//!
//! A directory is walked recursively with the SAME walk `check-project` uses
//! ([`crate::find_lute_files`]: byte-sorted, symlink aliases deduplicated), so
//! the files rewritten are exactly the files the project checks, in a
//! deterministic order. Each file is rewritten independently: a refused or
//! unreadable file is reported and the walk continues, so one draft with
//! structural errors does not leave the rest of the tree untagged.
//!
//! Exit codes: `0` when every file succeeded (changed or not), `1` when some
//! file was refused (`--force` on a `codesLocked:` or structurally broken
//! document), `2` on an I/O failure (an unreadable path or an unwritable
//! file) — the worst outcome across the files wins.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// How one file fared. Ordered by severity so a tree's exit code is the max.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    Unchanged,
    Changed,
    Refused,
    Failed,
}

impl Outcome {
    fn exit_code(self) -> ExitCode {
        match self {
            Outcome::Unchanged | Outcome::Changed => ExitCode::SUCCESS,
            Outcome::Refused => ExitCode::from(1),
            Outcome::Failed => ExitCode::from(2),
        }
    }
}

/// Where a file's messages go: a lone file keeps the historical bare
/// `lute: <message>` lines; inside a tree every line names its file, and a
/// file with nothing to do stays silent (the summary counts it).
#[derive(Clone, Copy)]
enum Scope<'a> {
    Single,
    Tree(&'a Path),
}

impl Scope<'_> {
    fn say(self, msg: &str) {
        match self {
            Scope::Single => println!("lute: {msg}"),
            Scope::Tree(file) => println!("lute: {}: {msg}", file.display()),
        }
    }

    /// A no-op report: printed for a lone file only.
    fn say_unchanged(self, msg: &str) {
        if let Scope::Single = self {
            println!("lute: {msg}");
        }
    }
}

/// The files a `tag`/`fix` invocation rewrites: `path` itself, or every
/// `.lute` file under it when it is a directory. `Err` is the exit code.
fn targets(path: &Path) -> Result<Option<Vec<PathBuf>>, ExitCode> {
    if !path.is_dir() {
        return Ok(None);
    }
    crate::find_lute_files(path).map(Some).map_err(|e| {
        let e = lute_manifest::io_reason(&e);
        eprintln!("lute: cannot walk {}: {e}", path.display());
        ExitCode::from(2)
    })
}

/// Run `one` over `path` (a file) or every `.lute` file under it (a
/// directory), then — for a directory — print `summary(changed_files,
/// total_files, units)` where `units` sums what each changed file reported.
fn over_path(
    path: &Path,
    mut one: impl FnMut(&Path, Scope<'_>) -> (Outcome, usize),
    summary: impl Fn(usize, usize, usize) -> String,
) -> ExitCode {
    let files = match targets(path) {
        Ok(Some(files)) => files,
        Ok(None) => return one(path, Scope::Single).0.exit_code(),
        Err(code) => return code,
    };
    let mut worst = Outcome::Unchanged;
    let (mut changed, mut units) = (0, 0);
    for file in &files {
        let (outcome, n) = one(file, Scope::Tree(file));
        if outcome == Outcome::Changed {
            changed += 1;
            units += n;
        }
        worst = worst.max(outcome);
    }
    println!("lute: {}", summary(changed, files.len(), units));
    worst.exit_code()
}

fn read(file: &Path) -> Option<String> {
    match std::fs::read_to_string(file) {
        Ok(t) => Some(t),
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot read {}: {e}", file.display());
            None
        }
    }
}

fn write(file: &Path, text: &str) -> bool {
    match std::fs::write(file, text) {
        Ok(()) => true,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot write {}: {e}", file.display());
            false
        }
    }
}

/// Back-fill a stable `code` into every untagged `:line` (dsl §12), rewriting
/// each file in place. A thin shell over [`lute_check::tag_document`] (the
/// pure core that owns the tagging logic): read, tag, and — only when at least
/// one line was tagged — write the result back, so an already-tagged document
/// is left byte-identical (idempotent).
///
/// With `--force`, FORCE-renumber instead ([`lute_check::retag_document`]):
/// every line's code is rewritten in clean document order — a drafting tool.
/// Refused when frontmatter declares `codesLocked:` (published codes are
/// `lineId`/`voiceKey` identity; renumbering severs the localization/voice
/// join) or when the document has structural errors.
pub fn run_tag(path: &Path, force: bool) -> ExitCode {
    over_path(
        path,
        |file, scope| tag_file(file, force, scope),
        |changed, total, lines| {
            let verb = if force { "renumbered" } else { "tagged" };
            format!("{verb} {lines} line(s) in {changed} of {total} file(s)")
        },
    )
}

fn tag_file(file: &Path, force: bool, scope: Scope<'_>) -> (Outcome, usize) {
    let Some(text) = read(file) else {
        return (Outcome::Failed, 0);
    };

    if force {
        return match lute_check::retag_document(&text) {
            lute_check::RetagOutcome::LockedInstances { text: out, added } => {
                if !write(file, &out) {
                    return (Outcome::Failed, 0);
                }
                eprintln!(
                    "lute: {} declares `codesLocked:` — refusing line-code renumber; \
                     added {added} component instance key(s)",
                    file.display()
                );
                (Outcome::Refused, added)
            }
            lute_check::RetagOutcome::Locked => {
                eprintln!(
                    "lute: {} declares `codesLocked:` — its codes are published identity \
                     (lineId/voiceKey); refusing to renumber. Remove the key or set it \
                     `false` to renumber a draft.",
                    file.display()
                );
                (Outcome::Refused, 0)
            }
            lute_check::RetagOutcome::Broken => {
                eprintln!(
                    "lute: {} has structural errors — fix `lute check` findings first; \
                     nothing was rewritten",
                    file.display()
                );
                (Outcome::Refused, 0)
            }
            lute_check::RetagOutcome::Renumbered {
                text: out,
                renumbered,
                skipped,
            } => {
                let outcome = if renumbered > 0 {
                    if !write(file, &out) {
                        return (Outcome::Failed, 0);
                    }
                    scope.say(&format!("renumbered {renumbered} line(s)"));
                    Outcome::Changed
                } else {
                    scope.say_unchanged("codes already in order");
                    Outcome::Unchanged
                };
                if skipped > 0 {
                    scope.say(&format!(
                        "{skipped} line(s) skipped (non-string `code` value)"
                    ));
                }
                (outcome, renumbered)
            }
        };
    }

    let out = lute_check::tag_document(&text);
    if out.added == 0 {
        scope.say_unchanged("already tagged");
        return (Outcome::Unchanged, 0);
    }
    if !write(file, &out.text) {
        return (Outcome::Failed, 0);
    }
    scope.say(&format!("tagged {} line(s)", out.added));
    (Outcome::Changed, out.added)
}

/// Apply `lute fix`'s mechanical migrations in place (dsl §7.1, §7.3, 0.18.0
/// §3 and 0.32.0 §1.3), rewriting a file only when a span was actually
/// changed. CEL slots are visited through the shared syntax walker; YAML
/// project surfaces use the same byte-preserving CEL rewriter.
pub fn run_fix(paths: &[PathBuf]) -> ExitCode {
    if let [path] = paths {
        if !path.is_dir() {
            let volume = volume_eligibility(std::slice::from_ref(path))[0];
            return fix_file(path, Scope::Single, volume).0.exit_code();
        }
    }
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            match fix_targets(path) {
                Ok(found) => files.extend(found),
                Err(e) => {
                    eprintln!("lute: cannot walk {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            }
        } else {
            files.push(path.clone());
        }
    }
    let volume = volume_eligibility(&files);
    let mut worst = Outcome::Unchanged;
    let mut changed = 0;
    let mut fixes = 0;
    for (file, &volume) in files.iter().zip(&volume) {
        let (outcome, n) = fix_file(file, Scope::Tree(file), volume);
        if outcome == Outcome::Changed {
            changed += 1;
            fixes += n;
        }
        worst = worst.max(outcome);
    }
    println!(
        "lute: applied {fixes} fix(es) in {changed} of {} file(s)",
        files.len()
    );
    worst.exit_code()
}

fn fix_targets(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            if p.is_dir() {
                visit(&p, out)?;
            } else if p
                .extension()
                .is_some_and(|ext| ext == "lute" || ext == "yaml" || ext == "yml")
            {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(path, &mut files)?;
    Ok(files)
}

fn fix_file(file: &Path, scope: Scope<'_>, volume: bool) -> (Outcome, usize) {
    let Some(text) = read(file) else {
        return (Outcome::Failed, 0);
    };
    let legacy = lute_check::fix_document(&text);
    let (mut out, mut changed) = (legacy.text, legacy.changed);
    let yaml = file
        .extension()
        .is_some_and(|ext| ext == "yaml" || ext == "yml");
    let cel = if yaml {
        rewrite_yaml_cel(&out)
    } else {
        rewrite_lute_cel(&out)
    };
    changed += cel.changed;
    out = cel.text;
    if yaml {
        let domains = rewrite_yaml_domains(&out);
        for (line, renamed) in &domains.kept {
            eprintln!(
                "lute fix: {}:{line}: kept `{}:` — `{}:` is already declared",
                file.display(),
                renamed.old,
                renamed.new
            );
        }
        changed += domains.rewrite.changed;
        out = domains.rewrite.text;
    }
    let stamp = rewrite_lute_version(&out, file);
    changed += stamp.changed;
    out = stamp.text;
    let bindings = rewrite_bindings_version(&out, file);
    changed += bindings.changed;
    out = bindings.text;
    if volume {
        let renamed = rewrite_volume(&out, yaml);
        changed += renamed.changed;
        out = renamed.text;
    }
    if changed == 0 {
        scope.say_unchanged("nothing to fix");
        return (Outcome::Unchanged, 0);
    }
    if !write(file, &out) {
        return (Outcome::Failed, 0);
    }
    scope.say(&format!("applied {changed} fix(es)"));
    (Outcome::Changed, changed)
}

/// Convert the 0.31 fact-query calls to standard CEL list-form calls. The
/// outer parser validates the complete slot; candidate spans are then found
/// lexically because cel-parser 0.10 does not retain successful node offsets.
#[cfg(test)]
fn rewrite_cel(raw: &str) -> Rewrite {
    rewrite_cel_with_quote(raw, '"')
}

fn rewrite_cel_with_quote(raw: &str, quote: char) -> Rewrite {
    let mut arena = lute_cel::CelArena::default();
    if lute_cel::parse_slot(&mut arena, raw, 0).is_err() {
        return Rewrite {
            text: raw.to_string(),
            changed: 0,
        };
    }
    let mut candidates = Vec::new();
    for (start, end) in call_spans(raw) {
        let name_end = raw[start..].find('(').unwrap_or(0) + start;
        let name = raw[start..name_end].trim();
        if !matches!(
            name,
            "holds" | "count" | "countDistinct" | "validAt" | "isSet"
        ) {
            continue;
        }
        let call = &raw[start..end];
        let mut call_arena = lute_cel::CelArena::default();
        let Ok(handle) = lute_cel::parse_slot(&mut call_arena, call, 0) else {
            continue;
        };
        let Some(root) = call_arena.get(handle) else {
            continue;
        };
        let Some(replacement) = rewrite_call(raw, start, end, name, &root.expr, quote) else {
            continue;
        };
        candidates.push((start, end, replacement));
    }
    // Keep an outer replacement when candidates overlap; an inner target call
    // is either part of that call's preserved expression or is not a valid
    // old-form argument shape.
    candidates.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    let mut selected = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|(start, end, _): &(usize, usize, String)| {
                candidate.0 < *end && candidate.1 > *start
            })
        {
            continue;
        }
        selected.push(candidate);
    }
    apply_edits(raw, selected)
}

fn call_spans(raw: &str) -> Vec<(usize, usize)> {
    let bytes = raw.as_bytes();
    let mask = lute_manifest::text::cel_string_mask(raw);
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if mask[i] || !is_ident_start(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() && is_ident_continue(bytes[i]) {
            i += 1;
        }
        let mut open = i;
        while open < bytes.len() && bytes[open].is_ascii_whitespace() {
            open += 1;
        }
        if open >= bytes.len() || bytes[open] != b'(' {
            continue;
        }
        let prev = raw[..start].chars().next_back();
        if prev == Some('.') {
            continue;
        }
        if let Some(end) = matching_delimiter(raw, open) {
            out.push((start, end));
        }
    }
    out
}

fn matching_delimiter(raw: &str, open: usize) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mask = lute_manifest::text::cel_string_mask(raw);
    let mut depth = 0;
    for i in open..bytes.len() {
        if mask[i] {
            continue;
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn rewrite_call(
    raw: &str,
    start: usize,
    end: usize,
    name: &str,
    expr: &cel_parser::ast::Expr,
    quote: char,
) -> Option<String> {
    use cel_parser::ast::Expr;
    let Expr::Call(call) = expr else {
        return None;
    };
    if call.target.is_some() {
        return None;
    }
    let args_text = split_call_args(&raw[start..end])?;
    match name {
        "holds" | "count" | "validAt" => {
            if args_text.len() != if name == "validAt" { 2 } else { 1 } {
                return None;
            }
            let Expr::Call(pattern) = &call.args.first()?.expr else {
                return None;
            };
            if pattern.target.is_some()
                || pattern.args.len() != split_inner_args(&args_text[0]).len()
            {
                return None;
            }
            let relation = pattern.func_name.as_str();
            if relation.is_empty() {
                return None;
            }
            let mut values = Vec::new();
            for (arg, source) in pattern.args.iter().zip(split_inner_args(&args_text[0])) {
                values.push(rewrite_fact_arg(arg, &source, quote)?);
            }
            let list = format!("[{}]", values.join(", "));
            if name == "validAt" {
                Some(format!(
                    "{}({}, {}, {})",
                    name,
                    cel_string(relation, quote),
                    list,
                    args_text[1].clone()
                ))
            } else {
                Some(format!(
                    "{}({}, {})",
                    name,
                    cel_string(relation, quote),
                    list
                ))
            }
        }
        "countDistinct" => {
            if args_text.len() != 2 {
                return None;
            }
            let Expr::Call(pattern) = &call.args.first()?.expr else {
                return None;
            };
            if pattern.target.is_some()
                || pattern.args.len() != split_inner_args(&args_text[0]).len()
            {
                return None;
            }
            let Expr::Ident(variable) = &call.args[1].expr else {
                return None;
            };
            if !variable.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            let mut column = None;
            let mut values = Vec::new();
            for (idx, (arg, source)) in pattern
                .args
                .iter()
                .zip(split_inner_args(&args_text[0]))
                .enumerate()
            {
                if matches!(&arg.expr, Expr::Ident(name) if name == variable) {
                    if column.replace(idx).is_some() {
                        return None;
                    }
                    values.push(cel_string("_", quote));
                } else {
                    values.push(rewrite_fact_arg(arg, &source, quote)?);
                }
            }
            let column = column?;
            Some(format!(
                "countDistinct({}, [{}], {})",
                cel_string(&pattern.func_name, quote),
                values.join(", "),
                column
            ))
        }
        "isSet" => {
            if call.args.len() != 1 || args_text.len() != 1 {
                return None;
            }
            let path = &call.args[0].expr;
            let source = args_text[0].as_str();
            let segments = lute_cel::path::static_path(path)?;
            if segments.len() < 2 {
                return None;
            }
            if let Some((key, parent)) = quoted_final_index(source) {
                let replacement = format!("{} in {}", cel_string(&key, quote), parent);
                if needs_grouping(raw, start, end) {
                    Some(format!("({replacement})"))
                } else {
                    Some(replacement)
                }
            } else {
                Some(format!("has({source})"))
            }
        }
        _ => None,
    }
}

fn split_call_args(call: &str) -> Option<Vec<String>> {
    let open = call.find('(')?;
    let close = call.rfind(')')?;
    let mut args = Vec::new();
    let mut nested = 0i32;
    let mut quote = None;
    let mut start = open + 1;
    let bytes = call.as_bytes();
    for i in (open + 1)..close {
        match (quote, bytes[i]) {
            (None, b'\'' | b'"') => quote = Some(bytes[i]),
            (Some(q), b) if b == q && (i == 0 || bytes[i - 1] != b'\\') => quote = None,
            (None, b'(' | b'[' | b'{') => nested += 1,
            (None, b')' | b']' | b'}') => nested -= 1,
            (None, b',') if nested == 0 => {
                let value = call[start..i].trim();
                if !value.is_empty() {
                    args.push(value.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let value = call[start..close].trim();
    if !value.is_empty() {
        args.push(value.to_string());
    }
    Some(args)
}

fn split_inner_args(call: &str) -> Vec<String> {
    split_call_args(call).unwrap_or_default()
}

fn rewrite_fact_arg(expr: &cel_parser::ast::IdedExpr, source: &str, quote: char) -> Option<String> {
    use cel_parser::ast::Expr;
    use cel_parser::reference::Val;
    if matches!(source.trim(), "true" | "false") {
        return Some(source.trim().to_string());
    }
    match &expr.expr {
        Expr::Ident(_) if source.starts_with('@') => Some(source.to_string()),
        Expr::Ident(name) if matches!(name.as_str(), "true" | "false") => Some(name.clone()),
        Expr::Literal(Val::String(value)) => Some(cel_string(value, quote)),
        Expr::Select(_) | Expr::Call(_) if lute_cel::path::static_path(&expr.expr).is_some() => {
            Some(source.to_string())
        }
        _ if source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.')) =>
        {
            Some(cel_string(source, quote))
        }
        _ => None,
    }
}

fn cel_string(value: &str, quote: char) -> String {
    format!("{quote}{value}{quote}")
}

fn quoted_final_index(source: &str) -> Option<(String, String)> {
    let source = source.trim();
    if !source.ends_with(']') {
        return None;
    }
    let mask = lute_manifest::text::cel_string_mask(source);
    let open = source
        .bytes()
        .enumerate()
        .rev()
        .find(|(i, b)| *b == b'[' && !mask[*i])?
        .0;
    let key_text = source[open + 1..source.len() - 1].trim();
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, key_text, 0).ok()?;
    let cel_parser::ast::Expr::Literal(cel_parser::reference::Val::String(key)) =
        &arena.get(handle)?.expr
    else {
        return None;
    };
    Some((key.to_string(), source[..open].trim().to_string()))
}

fn needs_grouping(raw: &str, start: usize, end: usize) -> bool {
    let before = raw[..start].trim_end().chars().next_back();
    let after = raw[end..].trim_start();
    before == Some('!')
        || after.starts_with('+')
        || after.starts_with('-')
        || after.starts_with('*')
        || after.starts_with('/')
        || after.starts_with('%')
        || after.starts_with('?')
        || after.starts_with(':')
}

#[derive(Default)]
struct Rewrite {
    text: String,
    changed: usize,
}

/// Rewrite every parsed CEL-bearing AST slot, applying replacements in reverse
/// byte order so comments and bytes outside a call remain untouched.
fn rewrite_lute_cel(text: &str) -> Rewrite {
    let (doc, _) = lute_syntax::parse(text);
    let mut slots = Vec::new();
    lute_syntax::walk::for_each_cel_slot(&doc, &mut |slot| {
        let start = slot.span.byte_start;
        let end = slot.span.byte_end;
        if end <= text.len() && start <= end && text.get(start..end) == Some(slot.raw.as_str()) {
            slots.push((start, end, slot.raw.clone()));
        }
    });
    let mut edits = Vec::new();
    for (start, end, raw) in slots {
        let quote = match (
            text.as_bytes().get(start.wrapping_sub(1)),
            text.as_bytes().get(end),
        ) {
            (Some(&b'\''), Some(&b'\'')) => '"',
            (Some(&b'"'), Some(&b'"')) => '\'',
            _ => '"',
        };
        let rewritten = rewrite_cel_with_quote(&raw, quote);
        if rewritten.text != raw {
            edits.push((start, end, rewritten.text));
        }
    }
    let mut result = apply_edits(text, edits);
    if let Some((start, end)) = frontmatter_range(&result.text) {
        let old = result.text[start..end].to_string();
        let fm = rewrite_yaml_cel(&old);
        if fm.text != old {
            result.text = format!(
                "{}{}{}",
                &result.text[..start],
                fm.text,
                &result.text[end..]
            );
            result.changed += fm.changed;
        }
    }
    rewrite_lute_attribute(result)
}

fn rewrite_lute_attribute(mut result: Rewrite) -> Rewrite {
    let mut text = result.text;
    let mut offset = 0;
    while let Some(rel) = text[offset..].find(" only=") {
        let start = offset + rel + 6;
        let Some(q) = text.as_bytes().get(start).copied() else {
            break;
        };
        if q != b'\'' && q != b'"' {
            offset = start;
            continue;
        }
        let end = match text[start + 1..].find(q as char) {
            Some(i) => start + 1 + i,
            None => break,
        };
        let raw = text[start + 1..end].to_string();
        let rewritten = rewrite_cel_with_quote(&raw, if q == b'"' { '\'' } else { '"' });
        if rewritten.text != raw {
            let replacement = rewritten.text;
            text.replace_range(start + 1..end, &replacement);
            offset = start + 1 + replacement.len();
            result.changed += 1;
        } else {
            offset = end + 1;
        }
    }
    result.text = text;
    result
}

fn frontmatter_range(text: &str) -> Option<(usize, usize)> {
    if !text.starts_with("---") {
        return None;
    }
    let first_end = text.find('\n')? + 1;
    let close_rel = text[first_end..].find("\n---")?;
    Some((first_end, first_end + close_rel + 4))
}

/// 0.41.1 §3.2: raise an existing language stamp older than the toolchain's
/// [`lute_check::LUTE_LANG_VERSION`] to it — `defaults.luteVersion` in a
/// `lute.project.yaml`, a `.lute` document's top-level frontmatter
/// `luteVersion`. Only the scalar's text is spliced (quotes, comments and
/// layout survive); a missing stamp is never added, and a current, newer or
/// non-`MAJOR.MINOR.PATCH` stamp is left alone, so a second run changes
/// nothing.
fn rewrite_lute_version(text: &str, file: &Path) -> Rewrite {
    let span = if file
        .file_name()
        .is_some_and(|name| name == "lute.project.yaml")
    {
        manifest_stamp_span(text)
    } else if file.extension().is_some_and(|ext| ext == "lute") {
        frontmatter_range(text).and_then(|(start, end)| {
            let (s, e) = top_level_scalar_span(&text[start..end], "luteVersion")?;
            Some((start + s, start + e))
        })
    } else {
        None
    };
    let current = lute_check::LUTE_LANG_VERSION;
    let older = |stamp: &str| {
        matches!(
            (lute_check::check::version_triple(stamp), lute_check::check::version_triple(current)),
            (Some(s), Some(c)) if s < c
        )
    };
    let edits = span
        .filter(|&(s, e)| older(&text[s..e]))
        .map(|(s, e)| (s, e, current.to_string()))
        .into_iter()
        .collect();
    apply_edits(text, edits)
}

/// 0.47.0 §4.2: raise the `bindingsVersion` of a `player.bindings.yaml` that
/// is an older `MAJOR.MINOR` than the toolchain line to that line. Only the
/// scalar's text is spliced (quotes, comments and layout survive); a current,
/// newer or non-`MAJOR.MINOR` value is left alone, so a second run changes
/// nothing.
fn rewrite_bindings_version(text: &str, file: &Path) -> Rewrite {
    let line = crate::bindings::toolchain_line();
    let span = file
        .file_name()
        .is_some_and(|name| name == "player.bindings.yaml")
        .then(|| top_level_scalar_span(text, "bindingsVersion"))
        .flatten();
    let older = |value: &str| {
        matches!(
            (crate::bindings::bindings_line(value), crate::bindings::bindings_line(&line)),
            (Some(v), Some(l)) if v < l
        )
    };
    let edits = span
        .filter(|&(s, e)| older(&text[s..e]))
        .map(|(s, e)| (s, e, line.clone()))
        .into_iter()
        .collect();
    apply_edits(text, edits)
}

/// The byte span of the scalar text of the top-level `key:` in a YAML
/// mapping written in block style (quotes excluded).
fn top_level_scalar_span(yaml: &str, key: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in yaml.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if let Some((0, end, value)) = yaml_key_value(body) {
            if yaml_key_is(&body[..end], key) {
                let (s, e) = yaml_scalar_span(body, value)?;
                return Some((offset + s, offset + e));
            }
        }
        offset += line.len();
    }
    None
}

/// The byte span of the scalar text of `defaults.luteVersion` in a project
/// manifest — a block mapping under the top-level `defaults:` key, or a flow
/// mapping `defaults: { luteVersion: … }` (quotes excluded).
fn manifest_stamp_span(text: &str) -> Option<(usize, usize)> {
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len();
            Some((at, line.strip_suffix('\n').unwrap_or(line)))
        })
        .collect();
    let (i, (line_at, body, value)) =
        lines
            .iter()
            .enumerate()
            .find_map(|(i, &(at, body))| match yaml_key_value(body) {
                Some((0, end, value)) if yaml_key_is(&body[..end], "defaults") => {
                    Some((i, (at, body, value)))
                }
                _ => None,
            })?;
    if body[value..].starts_with('{') {
        let (at, key) = flow_map_keys(text, line_at + value)
            .into_iter()
            .find(|(_, key)| *key == "luteVersion")?;
        let b = text.as_bytes();
        let mut v = at + key.len();
        // A quoted key's closing quote, then the `:` and its spaces.
        while v < b.len() && matches!(b[v], b'"' | b'\'' | b' ' | b'\t') {
            v += 1;
        }
        if b.get(v) != Some(&b':') {
            return None;
        }
        v += 1;
        while v < b.len() && matches!(b[v], b' ' | b'\t') {
            v += 1;
        }
        return match b.get(v) {
            Some(&q @ (b'"' | b'\'')) => {
                let close = text[v + 1..].find(q as char)? + v + 1;
                Some((v + 1, close))
            }
            _ => {
                let mut e = v;
                while e < b.len()
                    && !matches!(b[e], b',' | b'}' | b'#' | b'\n' | b'\r' | b' ' | b'\t')
                {
                    e += 1;
                }
                (e > v).then_some((v, e))
            }
        };
    }
    if yaml_value_end(body, value) != value {
        return None;
    }
    let mut child_indent = None;
    for &(offset, body) in &lines[i + 1..] {
        let trimmed = body.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = body.len() - trimmed.len();
        if indent == 0 {
            break;
        }
        if *child_indent.get_or_insert(indent) != indent {
            continue;
        }
        if let Some((start, end, value)) = yaml_key_value(body) {
            if start == indent && yaml_key_is(&body[start..end], "luteVersion") {
                let (s, e) = yaml_scalar_span(body, value)?;
                return Some((offset + s, offset + e));
            }
        }
    }
    None
}

/// Whether the YAML mapping key spelled `raw` — plain, `'single'` or
/// `"double"`-quoted — is `name`.
fn yaml_key_is(raw: &str, name: &str) -> bool {
    match raw.as_bytes() {
        [q @ (b'"' | b'\''), .., last] if last == q => &raw[1..raw.len() - 1] == name,
        _ => raw == name,
    }
}

/// The text of the plain or quoted YAML scalar starting at byte `start` of
/// `line` (quotes excluded); `None` for a collection, block scalar, anchor,
/// alias, tag, empty or unterminated value.
fn yaml_scalar_span(line: &str, start: usize) -> Option<(usize, usize)> {
    let end = yaml_value_end(line, start);
    let raw = line.get(start..end)?.trim_end();
    match raw.as_bytes() {
        [q @ (b'"' | b'\''), .., last] if last == q => Some((start + 1, start + raw.len() - 1)),
        [] | [b'"' | b'\'' | b'{' | b'[' | b'|' | b'>' | b'&' | b'*' | b'!', ..] => None,
        _ => Some((start, start + raw.len())),
    }
}

/// 0.42.0 §3.3: the documented `volume` scale before 0.42, in order — the
/// only folded `volume` domain `lute fix` migrates.
const OLD_VOLUME: [&str; 5] = ["silent", "down", "normal", "up", "full"];

/// The 0.42 member for an old relative-sounding `volume` member.
fn new_volume_member(old: &str) -> Option<&'static str> {
    match old {
        "down" => Some("quiet"),
        "up" => Some("loud"),
        _ => None,
    }
}

/// Whether each of `files` gets the 0.42 volume migration, decided before
/// any file is rewritten: its project's folded `volume` domain is exactly
/// [`OLD_VOLUME`] in every document that sees one, or — a `.lute` file
/// outside any project — its own frontmatter declares exactly that.
fn volume_eligibility(files: &[PathBuf]) -> Vec<bool> {
    let mut projects: BTreeMap<PathBuf, bool> = BTreeMap::new();
    files
        .iter()
        .map(|file| match project_root(file) {
            Some(root) => *projects
                .entry(root)
                .or_insert_with_key(|root| project_has_old_volume(root)),
            None => {
                file.extension().is_some_and(|ext| ext == "lute")
                    && read(file)
                        .is_some_and(|text| frontmatter_volume(&text).is_some_and(is_old_volume))
            }
        })
        .collect()
}

fn is_old_volume(members: Vec<String>) -> bool {
    members.iter().map(String::as_str).eq(OLD_VOLUME)
}

/// The nearest directory at or above `file` holding a `lute.project.yaml`.
fn project_root(file: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(file).ok()?;
    absolute
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("lute.project.yaml").is_file())
        .map(Path::to_path_buf)
}

/// Whether the project at `root` folds exactly [`OLD_VOLUME`] wherever a
/// document sees a `volume` domain (and at least one does). Documents that
/// disagree make the project ineligible: its declarations are not the old
/// scale alone.
fn project_has_old_volume(root: &Path) -> bool {
    let options = lute_model::ModelOptions {
        compile: false,
        ..Default::default()
    };
    let Ok(model) = lute_model::ProjectModel::build_single_root(root, &options) else {
        return false;
    };
    let mut seen = false;
    for document in model.documents() {
        if let Some(domain) = document.folded.domains.get("volume") {
            if !is_old_volume(domain.members.clone()) {
                return false;
            }
            seen = true;
        }
    }
    seen
}

/// A `.lute` document's own frontmatter `enums: volume:` members.
fn frontmatter_volume(text: &str) -> Option<Vec<String>> {
    let (start, end) = frontmatter_range(text)?;
    let yaml = text[start..end]
        .strip_suffix("\n---")
        .unwrap_or(&text[start..end]);
    let value: yaml_serde::Value = yaml_serde::from_str(yaml).ok()?;
    let volume = value.get("enums")?.get("volume")?;
    let members = volume.get("members").unwrap_or(volume);
    members
        .as_sequence()?
        .iter()
        .map(|m| m.as_str().map(str::to_string))
        .collect()
}

/// 0.42.0 §3.3: in a file whose project folds the old scale, rename `down`
/// → `quiet` and `up` → `loud` where they are `volume` members — the
/// `volume:` entry of a top-level `enums:` (a YAML file's, or a document's
/// frontmatter), the keys of a bindings file's top-level `volumes:`, and a
/// directive's `volume=` value. Spans are spliced in place; each renamed
/// token is one fix.
fn rewrite_volume(text: &str, yaml: bool) -> Rewrite {
    if yaml {
        return apply_edits(text, volume_yaml_edits(text, 0));
    }
    let mut edits = Vec::new();
    let body_from = match frontmatter_range(text) {
        Some((start, end)) => {
            edits.extend(volume_yaml_edits(&text[start..end], start));
            end
        }
        None => 0,
    };
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if offset >= body_from && line.trim_start().starts_with("::") {
            edits.extend(volume_attr_edits(line, offset));
        }
        offset += line.len();
    }
    apply_edits(text, edits)
}

/// The `volume=<member>` attribute values of one directive line.
fn volume_attr_edits(line: &str, offset: usize) -> Vec<(usize, usize, String)> {
    let bytes = line.as_bytes();
    let mut edits = Vec::new();
    let mut from = 0;
    while let Some(rel) = line[from..].find("volume=") {
        let at = from + rel;
        from = at + "volume=".len();
        if at > 0 && (is_ident_continue(bytes[at - 1]) || bytes[at - 1] == b'-') {
            continue;
        }
        let quote = bytes
            .get(from)
            .copied()
            .filter(|c| *c == b'"' || *c == b'\'');
        let start = from + usize::from(quote.is_some());
        let mut end = start;
        while end < bytes.len() && is_ident_continue(bytes[end]) {
            end += 1;
        }
        let closes = match quote {
            Some(q) => bytes.get(end) == Some(&q),
            None => bytes
                .get(end)
                .is_none_or(|c| !is_ident_continue(*c) && *c != b'-'),
        };
        if let (true, Some(new)) = (closes, new_volume_member(&line[start..end])) {
            edits.push((offset + start, offset + end, new.to_string()));
        }
    }
    edits
}

/// The old members in a YAML text's `enums: volume:` entry and its
/// `volumes:` keys, as edits at `base` + their offset.
fn volume_yaml_edits(text: &str, base: usize) -> Vec<(usize, usize, String)> {
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len();
            Some((at, line.strip_suffix('\n').unwrap_or(line)))
        })
        .collect();
    let mut edits = Vec::new();
    let mut push = |at: usize, token: &str| {
        if let Some(new) = new_volume_member(token) {
            edits.push((base + at, base + at + token.len(), new.to_string()));
        }
    };
    for (i, &(line_at, body)) in lines.iter().enumerate() {
        let Some((0, end, value)) = yaml_key_value(body) else {
            continue;
        };
        let section = &body[..end];
        let enums = yaml_key_is(section, "enums");
        if !enums && !yaml_key_is(section, "volumes") {
            continue;
        }
        // The entries of the section: (key offset, key, value region).
        let mut entries: Vec<(usize, &str, std::ops::Range<usize>)> = Vec::new();
        if body[value..].starts_with('{') {
            let close = line_at + value + balanced_end(&text[line_at + value..]);
            let keys = flow_map_keys(text, line_at + value);
            for (k, &(at, key)) in keys.iter().enumerate() {
                let next = keys.get(k + 1).map_or(close, |&(n, _)| n);
                entries.push((at, key, at + key.len()..next));
            }
        } else if yaml_value_end(body, value) == value {
            let mut child_indent = None;
            let mut j = i + 1;
            while j < lines.len() {
                let (at, child) = lines[j];
                let trimmed = child.trim_start();
                let indent = child.len() - trimmed.len();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    j += 1;
                    continue;
                }
                if indent == 0 {
                    break;
                }
                if *child_indent.get_or_insert(indent) != indent {
                    j += 1;
                    continue;
                }
                let Some((start, end, _)) = yaml_key_value(child) else {
                    j += 1;
                    continue;
                };
                let raw = &child[start..end];
                let (key, inner) = match raw.as_bytes() {
                    [q @ (b'"' | b'\''), .., last] if last == q => {
                        (&raw[1..raw.len() - 1], start + 1)
                    }
                    _ => (raw, start),
                };
                // The entry's value: the rest of its line and every deeper line.
                let mut stop = j + 1;
                while stop < lines.len() {
                    let (_, next) = lines[stop];
                    let t = next.trim_start();
                    if !t.is_empty() && !t.starts_with('#') && next.len() - t.len() <= indent {
                        break;
                    }
                    stop += 1;
                }
                let region_end = lines.get(stop).map_or(text.len(), |&(at, _)| at);
                entries.push((at + inner, key, at + end..region_end));
                j = stop;
            }
        }
        for (at, key, region) in entries {
            if enums && key == "volume" {
                for (token_at, token) in member_tokens(&text[region.clone()]) {
                    push(region.start + token_at, token);
                }
            } else if !enums {
                push(at, key);
            }
        }
    }
    edits
}

/// The byte length of the balanced `{…}` / `[…]` opening `text`.
fn balanced_end(text: &str) -> usize {
    let mut depth = 0usize;
    let mut quote = None;
    for (i, b) in text.bytes().enumerate() {
        match (quote, b) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'{' | b'[') => depth += 1,
            (None, b'}' | b']') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// The member references of a `volume:` declaration's value region, outside
/// comments: a list item (`[a, b]`, `- a`), a mapping key (`labels: { a: … }`)
/// and a `default:` value. A quoted string counts when its content is one
/// identifier. Other scalars — a `description:`, say — are prose.
fn member_tokens(region: &str) -> Vec<(usize, &str)> {
    let b = region.as_bytes();
    let ident = |c: u8| is_ident_continue(c) || c == b'-';
    let blank = |c: u8| c == b' ' || c == b'\t';
    // Is the token spanning `outer` (quotes included) a member position?
    let member = |outer: std::ops::Range<usize>| {
        let mut p = outer.start;
        while p > 0 && b[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        let prev = p.checked_sub(1).map(|i| b[i]);
        let mut n = outer.end;
        while n < b.len() && blank(b[n]) {
            n += 1;
        }
        let next = b.get(n).copied();
        let ends_item = matches!(next, None | Some(b',' | b']' | b'#' | b'\n' | b'\r'));
        let dash_item = prev == Some(b'-')
            && region[..p - 1]
                .bytes()
                .rev()
                .take_while(|c| *c != b'\n')
                .all(blank);
        let default_value = prev == Some(b':') && region[..p - 1].trim_end().ends_with("default");
        next == Some(b':')
            || (ends_item && (matches!(prev, Some(b'[' | b',')) || dash_item))
            || default_value
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' if i == 0 || b[i - 1].is_ascii_whitespace() => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            q @ (b'"' | b'\'') => {
                let start = i + 1;
                let mut end = start;
                while end < b.len() && b[end] != q && b[end] != b'\n' {
                    end += 1;
                }
                let one = end > start && region[start..end].bytes().all(ident);
                if one && member(i..(end + 1).min(b.len())) {
                    out.push((start, &region[start..end]));
                }
                i = end + 1;
            }
            c if ident(c) => {
                let start = i;
                while i < b.len() && ident(b[i]) {
                    i += 1;
                }
                if member(start..i) {
                    out.push((start, &region[start..i]));
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn apply_edits(text: &str, mut edits: Vec<(usize, usize, String)>) -> Rewrite {
    edits.sort_by_key(|(start, _, _)| *start);
    let mut out = text.to_string();
    let mut changed = 0;
    for (start, end, replacement) in edits.into_iter().rev() {
        if start <= end && end <= out.len() && out.get(start..end).is_some() {
            out.replace_range(start..end, &replacement);
            changed += 1;
        }
    }
    Rewrite { text: out, changed }
}

/// Rewrite CEL-valued YAML scalars without serializing the YAML document. This
/// intentionally recognizes the manifest/schema condition keys and `defs:`
/// bodies, plus Datalog `cel("…")` guards in rule strings.
fn rewrite_yaml_cel(text: &str) -> Rewrite {
    let mut edits = Vec::new();
    let mut in_defs_indent = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let indent = body.len() - body.trim_start().len();
        let trimmed = body.trim_start();
        if trimmed.starts_with("defs:") {
            in_defs_indent = Some(indent);
        } else if in_defs_indent.is_some_and(|base| indent <= base) && !trimmed.is_empty() {
            in_defs_indent = None;
        }
        if trimmed.contains("present:")
            || trimmed.contains("assume:")
            || trimmed.contains("raisedWhen:")
        {
            if let Some(replacement) = rewrite_yaml_quoted_segments(body) {
                if replacement != body {
                    edits.push((offset, offset + body.len(), replacement));
                    offset += line.len();
                    continue;
                }
            }
        }
        if let Some((key_start, key_end, value_start)) = yaml_key_value(body) {
            let key = &body[key_start..key_end];
            let key_cel = matches!(
                key,
                "cel"
                    | "terminal"
                    | "live"
                    | "raisedWhen"
                    | "when"
                    | "condition"
                    | "test"
                    | "done"
                    | "visibleWhen"
                    | "by"
                    | "until"
                    | "start"
                    | "fail"
                    | "rearm"
                    | "spentBy"
                    | "guard"
                    | "present"
                    | "assume"
            ) || in_defs_indent.is_some_and(|base| indent > base && !key.is_empty());
            let value_end = yaml_value_end(body, value_start);
            if key_cel && !body[value_start..value_end].starts_with(['"', '\'']) {
                if let Some(replacement) = rewrite_yaml_quoted_segments(body) {
                    if replacement != body {
                        edits.push((offset, offset + body.len(), replacement));
                        offset += line.len();
                        continue;
                    }
                }
            }
            if value_start < value_end
                && (key_cel || body[value_start..value_end].contains("cel(\""))
            {
                if let Some(replacement) =
                    rewrite_yaml_scalar(&body[value_start..value_end], key_cel)
                {
                    if replacement != body[value_start..value_end] {
                        edits.push((offset + value_start, offset + value_end, replacement));
                    }
                }
            }
        } else if trimmed.contains("cel(\"") {
            // Sequence entries such as `- 'rel(x) :- cel("run.day > 0")'`.
            if let Some(replacement) = rewrite_yaml_quoted_segments(body) {
                if replacement != body {
                    edits.push((offset, offset + body.len(), replacement));
                }
            }
        }
        offset += line.len();
    }
    apply_edits(text, edits)
}

/// The YAML domain rename pass's result: the rewritten text and, per old
/// domain key kept because its mapping already declares the new name, the
/// key's 1-based line.
struct DomainRewrite {
    rewrite: Rewrite,
    kept: Vec<(usize, &'static lute_check::renames::RenamedDomain)>,
}

/// Rename renamed domains (`lute_check::renames::RENAMED_DOMAINS`) in project,
/// schema and plugin YAML (0.41.0 §3.1): a mapping key directly under a
/// top-level `enums:` / `entities:` block, and the scalar of every `domain:`
/// type reference. Spans are spliced in place like [`rewrite_yaml_cel`], so
/// comments, key order, quoting and layout survive; a second run changes
/// nothing. An old key whose mapping already declares the new name is kept
/// and reported instead (renaming it would duplicate the key).
fn rewrite_yaml_domains(text: &str) -> DomainRewrite {
    use lute_check::renames::RENAMED_DOMAINS;
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let at = *offset;
            *offset += line.len();
            Some((at, line.strip_suffix('\n').unwrap_or(line)))
        })
        .collect();
    let mut edits = Vec::new();
    let mut kept = Vec::new();

    // Keys of a top-level `enums:` / `entities:` mapping — a block at its
    // first child indent, or a flow map `{ … }` at its first level:
    // (1-based line, unquoted key, key byte offset in the text).
    let mut rename_keys = |keys: &[(usize, &str, usize)]| {
        for renamed in RENAMED_DOMAINS {
            let declares_new = keys.iter().any(|(_, key, _)| *key == renamed.new);
            for &(line, key, at) in keys {
                if key != renamed.old {
                    continue;
                }
                if declares_new {
                    kept.push((line, renamed));
                } else {
                    edits.push((at, at + key.len(), renamed.new.to_string()));
                }
            }
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let (line_at, body) = lines[i];
        i += 1;
        let Some((start, end, value)) = yaml_key_value(body) else {
            continue;
        };
        if start != 0 || !matches!(&body[start..end], "enums" | "entities") {
            continue;
        }
        if body[value..].starts_with('{') {
            let keys: Vec<(usize, &str, usize)> = flow_map_keys(text, line_at + value)
                .into_iter()
                .map(|(at, key)| (text[..at].matches('\n').count() + 1, key, at))
                .collect();
            rename_keys(&keys);
            continue;
        }
        if yaml_value_end(body, value) != value {
            continue;
        }
        let mut child_indent = None;
        let mut keys = Vec::new();
        while i < lines.len() {
            let (offset, body) = lines[i];
            let trimmed = body.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            let indent = body.len() - trimmed.len();
            if indent == 0 {
                break;
            }
            i += 1;
            if *child_indent.get_or_insert(indent) != indent {
                continue;
            }
            if let Some((start, end, _)) = yaml_key_value(body) {
                let raw = &body[start..end];
                let (key, inner) = match raw.as_bytes() {
                    [q @ (b'"' | b'\''), .., last] if last == q => {
                        (&raw[1..raw.len() - 1], start + 1)
                    }
                    _ => (raw, start),
                };
                keys.push((i, key, offset + inner));
            }
        }
        rename_keys(&keys);
    }

    // `domain: <old>` scalars, block or flow style, outside quotes/comments.
    for &(offset, body) in &lines {
        let bytes = body.as_bytes();
        let mut quote = None;
        let mut j = 0;
        while j < bytes.len() {
            let b = bytes[j];
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'\'' || b == b'"' => quote = Some(b),
                None if b == b'#' && (j == 0 || bytes[j - 1].is_ascii_whitespace()) => break,
                None if bytes[j..].starts_with(b"domain:")
                    && (j == 0 || !is_ident_continue(bytes[j - 1])) =>
                {
                    let mut v = j + "domain:".len();
                    while v < bytes.len() && bytes[v] == b' ' {
                        v += 1;
                    }
                    let quoted = bytes.get(v).copied().filter(|c| *c == b'"' || *c == b'\'');
                    let start = v + usize::from(quoted.is_some());
                    let mut end = start;
                    while end < bytes.len() && is_ident_continue(bytes[end]) {
                        end += 1;
                    }
                    let closes = match quoted {
                        Some(q) => bytes.get(end) == Some(&q),
                        None => bytes.get(end).is_none_or(|c| {
                            matches!(c, b' ' | b'\t' | b',' | b'}' | b']' | b'#' | b'\r')
                        }),
                    };
                    if closes {
                        if let Some(renamed) =
                            RENAMED_DOMAINS.iter().find(|r| r.old == &body[start..end])
                        {
                            edits.push((offset + start, offset + end, renamed.new.to_string()));
                        }
                    }
                    j = end + usize::from(quoted.is_some() && closes);
                    continue;
                }
                None => {}
            }
            j += 1;
        }
    }
    DomainRewrite {
        rewrite: apply_edits(text, edits),
        kept,
    }
}

/// The first-level keys of the YAML flow mapping opening at byte `open` of
/// `text` (`{ a: [x], "b": { c: y } }` → `a`, `b`), each with its unquoted
/// byte offset. Nested mappings and sequences, quoted scalars and comments
/// are skipped, so a nested value is never taken for a key.
fn flow_map_keys(text: &str, open: usize) -> Vec<(usize, &str)> {
    let b = text.as_bytes();
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut expect_key = false;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => {
                let start = i + 1;
                i = start;
                while i < b.len() && b[i] != q {
                    i += if q == b'"' && b[i] == b'\\' { 2 } else { 1 };
                }
                if expect_key && depth == 1 {
                    keys.push((start, &text[start..i.min(b.len())]));
                    expect_key = false;
                }
            }
            b'#' if b[i - 1].is_ascii_whitespace() => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            c @ (b'{' | b'[') => {
                depth += 1;
                expect_key = depth == 1 && c == b'{';
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => expect_key = true,
            c if c.is_ascii_whitespace() => {}
            _ if expect_key && depth == 1 => {
                let start = i;
                while i < b.len()
                    && !matches!(b[i], b':' | b',' | b'}')
                    && !b[i].is_ascii_whitespace()
                {
                    i += 1;
                }
                keys.push((start, &text[start..i]));
                expect_key = false;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    keys
}

fn rewrite_yaml_quoted_segments(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut edits = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\'' && bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let quote = bytes[i];
        let start = i;
        i += 1;
        while i < bytes.len() {
            if quote == b'"' && bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == quote {
                let end = i + 1;
                let raw = &line[start..end];
                let decoded = if quote == b'\'' {
                    raw[1..raw.len() - 1].replace("''", "'")
                } else {
                    yaml_serde::from_str::<String>(raw).ok()?
                };
                let cel_quote = if decoded.contains("cel(\"") {
                    '\''
                } else if quote == b'\'' {
                    '"'
                } else {
                    '\''
                };
                let rewritten = if decoded.contains("cel(\"") {
                    rewrite_rule_guards(&decoded, cel_quote).unwrap_or(decoded.clone())
                } else {
                    rewrite_cel_with_quote(&decoded, cel_quote).text
                };
                if rewritten != decoded {
                    let value = if quote == b'\'' {
                        format!("'{}'", rewritten.replace('\'', "''"))
                    } else {
                        format!(
                            "\"{}\"",
                            rewritten.replace('\\', "\\\\").replace('"', "\\\"")
                        )
                    };
                    edits.push((start, end, value));
                }
                i = end;
                break;
            } else {
                i += 1;
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    let mut out = line.to_string();
    for (start, end, value) in edits.into_iter().rev() {
        out.replace_range(start..end, &value);
    }
    Some(out)
}

fn yaml_key_value(line: &str) -> Option<(usize, usize, usize)> {
    let mut quote = None;
    for (i, b) in line.bytes().enumerate() {
        match (quote, b) {
            (None, b'\'' | b'"') => quote = Some(b),
            (Some(q), b) if b == q => quote = None,
            (None, b':') => {
                let key_start = line[..i].rfind([' ', '\t']).map_or(0, |x| x + 1);
                let key = line[key_start..i].trim();
                let start = i + 1 + line[i + 1..].len() - line[i + 1..].trim_start().len();
                return (!key.is_empty()).then_some((key_start, key_start + key.len(), start));
            }
            _ => {}
        }
    }
    None
}

fn yaml_value_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    if start >= bytes.len() {
        return start;
    }
    let quote = matches!(bytes[start], b'\'' | b'"').then_some(bytes[start]);
    if let Some(q) = quote {
        let mut i = start + 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' && q == b'"' {
                i += 2;
            } else if bytes[i] == q {
                return i + 1;
            } else {
                i += 1;
            }
        }
        return bytes.len();
    }
    let mut quote = None;
    for i in start..bytes.len() {
        match (quote, bytes[i]) {
            (None, b'\'' | b'"') => quote = Some(bytes[i]),
            (Some(q), b) if b == q => quote = None,
            (None, b'#') if i == start || bytes[i - 1].is_ascii_whitespace() => return i,
            _ => {}
        }
    }
    bytes.len()
}

fn rewrite_yaml_scalar(raw: &str, cel_key: bool) -> Option<String> {
    let (decoded, quote) = if raw.starts_with('"') && raw.ends_with('"') {
        (yaml_serde::from_str::<String>(raw).ok()?, Some(b'"'))
    } else if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        (raw[1..raw.len() - 1].replace("''", "'"), Some(b'\''))
    } else {
        (raw.trim().to_string(), None)
    };
    let cel_quote = if cel_key {
        match quote {
            Some(b'\'') => '"',
            Some(b'"') => '\'',
            _ => '"',
        }
    } else {
        '\''
    };
    let rewritten = if cel_key {
        rewrite_cel_with_quote(&decoded, cel_quote)
    } else {
        rewrite_rule_guards(&decoded, cel_quote).map_or(
            Rewrite {
                text: decoded.clone(),
                changed: 0,
            },
            |s| Rewrite {
                changed: usize::from(s != decoded),
                text: s,
            },
        )
    };
    (rewritten.text != decoded).then(|| match quote {
        Some(b'\'') => format!("'{}'", rewritten.text.replace('\'', "''")),
        Some(b'"') => format!(
            "\"{}\"",
            rewritten.text.replace('\\', "\\\\").replace('"', "\\\"")
        ),
        _ => format!("'{}'", rewritten.text.replace('\'', "''")),
    })
}

fn rewrite_rule_guards(text: &str, quote: char) -> Option<String> {
    let mut out = text.to_string();
    let mut at = 0;
    let mut changed = false;
    while let Some(rel) = out[at..].find("cel(\"") {
        let start = at + rel + 5;
        let end = out[start..].find('"')? + start;
        let body = &out[start..end];
        let next = rewrite_cel_with_quote(body, quote);
        if next.text != body {
            out.replace_range(start..end, &next.text);
            changed = true;
            at = start + next.text.len();
        } else {
            at = end + 1;
        }
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cel(raw: &str) -> String {
        rewrite_cel(raw).text
    }

    #[test]
    fn rewrites_already_quoted_fact_args_once() {
        assert_eq!(cel("holds(rel('edith'))"), "holds(\"rel\", [\"edith\"])");
        assert_eq!(cel("holds(rel(\"edith\"))"), "holds(\"rel\", [\"edith\"])");
    }

    #[test]
    fn rewrites_fact_query_arguments_and_nested_calls() {
        assert_eq!(
            cel("holds(rel(elena, '001', true, occasion.target, _)) && count(rel(_))"),
            "holds(\"rel\", [\"elena\", \"001\", true, occasion.target, \"_\"]) && count(\"rel\", [\"_\"])"
        );
        assert_eq!(
            cel("!(holds(rel(a, b)))"),
            "!(holds(\"rel\", [\"a\", \"b\"]))"
        );
    }
    #[test]
    fn rewrites_count_distinct_column_and_valid_at_expression() {
        assert_eq!(
            cel("countDistinct(sawAt(W, _, x), W)"),
            "countDistinct(\"sawAt\", [\"_\", \"_\", \"x\"], 0)"
        );
        assert_eq!(
            cel("validAt(at(hall), now() + 2)"),
            "validAt(\"at\", [\"hall\"], now() + 2)"
        );
    }

    #[test]
    fn rewrites_presence_and_preserves_precedence() {
        assert_eq!(cel("isSet(run.tip)"), "has(run.tip)");
        assert_eq!(
            cel("isSet(run.visits[\"lab-b2\"]) || run.day == 2"),
            "\"lab-b2\" in run.visits || run.day == 2"
        );
    }

    #[test]
    fn leaves_unmatched_shapes_and_second_run_unchanged() {
        let source = "holds(rel(a, b), extra) /* keep */";
        assert_eq!(cel(source), source);
        let once = cel("holds(rel(a, b))");
        assert_eq!(cel(&once), once);
    }

    #[test]
    fn rewrites_yaml_scalars_without_comments() {
        let source = "defs:\n  ready: \"holds(rel(a, b))\" # keep this comment\n";
        let out = rewrite_yaml_cel(source);
        assert!(out
            .text
            .contains(r#"ready: "holds('rel', ['a', 'b'])" # keep this comment"#));
        assert_eq!(rewrite_yaml_cel(&out.text).text, out.text);
    }
    #[test]
    fn rewrites_plain_yaml_scalars_and_returns_the_computed_value() {
        let out = rewrite_yaml_cel("defs:\n  ready: holds(rel(a, b))\n");
        assert!(out.text.contains("ready: 'holds(\"rel\", [\"a\", \"b\"])'"));
        assert_eq!(rewrite_yaml_cel(&out.text).text, out.text);
    }

    #[test]
    fn attribute_quote_is_safe() {
        let t = "<entry id=\"x\" when=\"holds(rel(a, b))\">\n</entry>\n";
        assert_eq!(
            rewrite_lute_cel(t).text,
            "<entry id=\"x\" when=\"holds('rel', ['a', 'b'])\">\n</entry>\n"
        );
    }

    #[test]
    fn rewrites_inline_yaml_raised_when_and_beat_only_attributes() {
        let yaml = r#"occasions:
  talk: { raisedWhen: "holds(fell(occasion.target))" }
"#;
        assert!(rewrite_yaml_cel(yaml)
            .text
            .contains(r#"raisedWhen: "holds('fell', [occasion.target])""#));
        let lute = r#"<beat id="x" only="holds(fell(occasion.target))">
</beat>
"#;
        assert!(rewrite_lute_cel(lute)
            .text
            .contains(r#"only="holds('fell', [occasion.target])""#));
    }

    #[test]
    fn rewrites_rule_guard_to_yaml_and_datalog_round_trip() {
        let source = "rules:\n  - 'foo(a) :- cel(\"holds(rel(a, b))\")'\n";
        let out = rewrite_yaml_cel(source);
        let yaml: yaml_serde::Value = yaml_serde::from_str(&out.text).expect("YAML");
        let body = yaml["rules"][0].as_str().expect("rule string");
        assert!(lute_syntax::datalog::parse_rule(body).is_ok(), "{body}");
        assert_eq!(rewrite_yaml_cel(&out.text).text, out.text);
    }
}

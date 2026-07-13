//! dsl 0.5.1 §1.1: the set of reserved `quest.<id>.state`/
//! `quest.<id>.objectives.<oid>.done` paths a document actually
//! REFERENCES — in a `<match on=…>` subject, a `<when>`/`when=` guard, an
//! interpolation, or any other CEL slot
//! ([`lute_syntax::walk::for_each_cel_slot`]'s canonical pre-order visits
//! every one, exhaustively). [`crate::mock::validate`] admits a `--state`
//! mock on a reserved path iff it is a member of this set — narrowing
//! `E-TRACE-MOCK-UNDECLARED` so a writer can preview the arm a
//! reserved-path read selects (§1.1) without `trace` gaining a
//! cross-document quest catalog.

use std::collections::BTreeSet;

use cel_parser::ast::{EntryExpr, Expr};
use lute_cel::CelArena;
use lute_syntax::ast::Document;

use crate::eval::{expr_path, is_reserved_quest_path};

/// Every reserved quest path referenced anywhere in `doc` (§1.1). Each
/// [`lute_syntax::ast::CelSlot`]'s `raw` text is re-parsed fresh into a
/// scratch [`CelArena`] — mirrors [`crate::walk`]'s own "never trust
/// `slot.ast`" idiom (its private `slot_expr`), kept local here rather than
/// depending on `validate`'s pipeline position (it happens to run BEFORE
/// `normalize`/`expand` ever touch `doc`, but re-parsing avoids this
/// module silently relying on that ordering fact). A slot that fails to
/// parse (gate-proven unreachable post-`check()`) contributes nothing; an
/// empty/whitespace-only `raw` (a structural gap) is skipped without
/// attempting to parse it.
pub(crate) fn collect_referenced_reserved_quest_paths(doc: &Document) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        let raw = slot.raw.trim();
        if raw.is_empty() {
            return;
        }
        let mut arena = CelArena::default();
        let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, 0) else {
            return;
        };
        let Some(rec) = arena.get(handle) else {
            return;
        };
        collect_paths(&rec.expr, &mut out);
    });
    out
}

/// Collect every maximal RESERVED quest path referenced in `expr`,
/// recursing into every sub-expression (call args, list/map/struct
/// elements, comprehensions) — mirrors `lute-check/src/cel_paths.rs`'s
/// `walk` (`pub(crate)` there, so not reusable across the D1 quarantine
/// boundary), simplified: this module only needs PRESENCE, never the
/// guard/read role distinction `check`'s definite-assignment pass needs.
fn collect_paths(expr: &Expr, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Ident(_) => {
            if let Some(path) = expr_path(expr) {
                if is_reserved_quest_path(&path) {
                    out.insert(path);
                }
            }
        }
        Expr::Select(sel) => {
            if let Some(path) = expr_path(expr) {
                if is_reserved_quest_path(&path) {
                    out.insert(path);
                }
            } else {
                // Chain bottoms out in a non-ident (e.g. `f(x).field`): not
                // a static path, but its operand may still contain reads.
                collect_paths(&sel.operand.expr, out);
            }
        }
        Expr::Call(call) => {
            if let Some(target) = &call.target {
                collect_paths(&target.expr, out);
            }
            for arg in &call.args {
                collect_paths(&arg.expr, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                collect_paths(&el.expr, out);
            }
        }
        Expr::Map(map) => {
            for entry in &map.entries {
                collect_entry(&entry.expr, out);
            }
        }
        Expr::Struct(st) => {
            for entry in &st.entries {
                collect_entry(&entry.expr, out);
            }
        }
        Expr::Comprehension(c) => {
            collect_paths(&c.iter_range.expr, out);
            collect_paths(&c.accu_init.expr, out);
            collect_paths(&c.loop_cond.expr, out);
            collect_paths(&c.loop_step.expr, out);
            collect_paths(&c.result.expr, out);
        }
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn collect_entry(entry: &EntryExpr, out: &mut BTreeSet<String>) {
    match entry {
        EntryExpr::MapEntry(m) => {
            collect_paths(&m.key.expr, out);
            collect_paths(&m.value.expr, out);
        }
        EntryExpr::StructField(f) => collect_paths(&f.value.expr, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_for(text: &str) -> Document {
        let (doc, diags) = lute_syntax::parse(text);
        assert!(diags.is_empty(), "fixture must parse clean: {diags:?}");
        doc
    }

    #[test]
    fn collects_match_subject_and_guard_reads() {
        let doc = doc_for(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             <match on=\"quest.foo.state\">\n\
             <when is=\"active\" test=\"quest.foo.objectives.bar.done\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n\
             </match>\n",
        );
        let refs = collect_referenced_reserved_quest_paths(&doc);
        assert!(refs.contains("quest.foo.state"), "{refs:?}");
        assert!(refs.contains("quest.foo.objectives.bar.done"), "{refs:?}");
        assert_eq!(refs.len(), 2, "{refs:?}");
    }

    #[test]
    fn ignores_ordinary_declared_paths_and_unreferenced_reserved_paths() {
        let doc = doc_for(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             state:\n  run.flag: { type: bool, default: false }\n---\n## Shot 1.\n\
             <match on=\"run.flag\">\n<when is=\"true\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n",
        );
        let refs = collect_referenced_reserved_quest_paths(&doc);
        assert!(refs.is_empty(), "{refs:?}");
    }
}

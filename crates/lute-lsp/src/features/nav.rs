//! `textDocument/definition` + `textDocument/references` (Task 6.3).
//!
//! Pure functions over a parsed [`Document`] + [`CapabilitySnapshot`] + byte
//! offset. [`definition_at`] jumps:
//! - an `@ref` -> its `defs:` decl site (or `None` for a snapshot-only def, which
//!   has no in-document site);
//! - a state path -> its `state:` decl;
//! - `scene.choices.<id>` -> the declaring `<branch id=…>` node.
//!
//! [`references_at`] returns every use site:
//! - an `@ref` -> all `@name` uses across the document's CEL slots;
//! - a state / choice path -> every `::set` target + CEL occurrence.
//!
//! ## Returned spans are byte-only
//! Both return [`Span`]s whose byte offsets are authoritative; `line`/`column`/
//! `utf16_range` may be zero for spans synthesized from the frontmatter YAML. The
//! backend re-derives the `Range` from the bytes through its `TextIndex`, so the
//! reported positions match the headless surface to the code unit.

use lute_core_span::Span;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

use super::{
    branch_span, choice_id, def_decl_span, is_state_path, path_at, path_uses, ref_at, ref_uses,
    state_decl_span, Cursor,
};

/// The declaration site of the symbol at byte offset `off`, or `None` when the
/// cursor is not on a navigable symbol (or the symbol has no in-document site).
pub fn definition_at(doc: &Document, snapshot: &CapabilitySnapshot, off: usize) -> Option<Span> {
    let cursor = super::resolve(doc, off)?;
    match cursor {
        Cursor::SetPath { path, .. } => path_definition(doc, path),
        Cursor::Cel { slot, .. } => {
            if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    None
                } else {
                    def_decl_span(doc, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                path_definition(doc, &tok)
            } else {
                None
            }
        }
        _ => {
            let _ = snapshot;
            None
        }
    }
}

/// A state/choice path's decl site: `scene.choices.<id>` -> the `<branch>` node;
/// any other declared state path -> its `state:` key.
fn path_definition(doc: &Document, path: &str) -> Option<Span> {
    if let Some(id) = choice_id(path) {
        if let Some(sp) = branch_span(doc, id) {
            return Some(sp);
        }
    }
    if is_state_path(path) {
        return state_decl_span(doc, path);
    }
    None
}

/// Every use site of the symbol at byte offset `off`. Empty when the cursor is not
/// on a referable symbol.
pub fn references_at(doc: &Document, snapshot: &CapabilitySnapshot, off: usize) -> Vec<Span> {
    let Some(cursor) = super::resolve(doc, off) else { return Vec::new() };
    let _ = snapshot;
    match cursor {
        Cursor::SetPath { path, .. } => path_uses(doc, path),
        Cursor::Cel { slot, .. } => {
            if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    Vec::new()
                } else {
                    ref_uses(doc, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                path_uses(doc, &tok)
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::TextIndex;
    use lute_manifest::core::load_core_snapshot;
    use lute_syntax::parse;

    fn parsed(text: &str) -> Document {
        parse(text).0
    }

    /// 1-based source line of a byte offset (matches `Span.line`).
    fn line_of(text: &str, byte: usize) -> u32 {
        TextIndex::new(text).position(byte).line
    }

    /// A Bianca-shaped fixture: a `<branch id="number">` plus a `<match on=…>` on
    /// its folded choice path, and a `@fond` def used in one arm.
    const BIANCA: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n## Shot 1.\n<branch id=\"number\">\n  <choice id=\"blunt\" label=\"Flat\">\n    :line[fixer]: number.\n  </choice>\n  <choice id=\"soft\" label=\"Gentle\">\n    ::set{scene.affect.bianca += 1}\n  </choice>\n</branch>\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :line[fixer]: gently.\n  </when>\n  <otherwise>\n    :line[fixer]: bluntly.\n  </otherwise>\n</match>\n";

    #[test]
    fn definition_on_choices_path_jumps_to_branch() {
        let doc = parsed(BIANCA);
        // Cursor inside the `on="scene.choices.number"` subject path.
        let off = BIANCA.find("scene.choices.number").unwrap() + 5;
        let loc = definition_at(&doc, &load_core_snapshot(), off).unwrap();
        let branch_line = line_of(BIANCA, BIANCA.find("<branch id=").unwrap());
        assert_eq!(line_of(BIANCA, loc.byte_start), branch_line);
    }

    #[test]
    fn definition_on_ref_jumps_to_def_decl() {
        let doc = parsed(BIANCA);
        let off = BIANCA.find("@fond").unwrap() + 1;
        let loc = definition_at(&doc, &load_core_snapshot(), off).unwrap();
        let def_line = line_of(BIANCA, BIANCA.find("  fond:").unwrap() + 2);
        assert_eq!(line_of(BIANCA, loc.byte_start), def_line);
    }

    #[test]
    fn definition_on_state_path_jumps_to_decl() {
        let doc = parsed(BIANCA);
        // The `::set{scene.affect.bianca += 1}` target path.
        let set_at = BIANCA.find("::set{").unwrap();
        let off = BIANCA[set_at..].find("scene.affect.bianca").unwrap() + set_at + 2;
        let loc = definition_at(&doc, &load_core_snapshot(), off).unwrap();
        let decl_line = line_of(BIANCA, BIANCA.find("  scene.affect.bianca:").unwrap() + 2);
        assert_eq!(line_of(BIANCA, loc.byte_start), decl_line);
    }

    #[test]
    fn references_on_ref_used_twice_returns_two() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\ndefs:\n  fond: { type: bool, cel: \"scene.x >= 1\" }\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :line[f]: a.\n  </when>\n  <when test=\"@fond && true\">\n    :line[f]: b.\n  </when>\n  <otherwise>\n    :line[f]: c.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@fond").unwrap() + 1; // on the first use
        let refs = references_at(&doc, &load_core_snapshot(), off);
        assert_eq!(refs.len(), 2, "two @fond uses: {refs:?}");
        // Both spans land on `@fond` occurrences.
        for r in &refs {
            assert_eq!(&text[r.byte_start..r.byte_end], "@fond");
        }
    }

    #[test]
    fn references_on_state_path_counts_set_and_reads() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca += 1}\n<match on=\"scene.affect.bianca\">\n  <otherwise>\n    :line[f]: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("scene.affect.bianca").unwrap() + set_at + 2;
        let refs = references_at(&doc, &load_core_snapshot(), off);
        // One `::set` target path + one `<match on=…>` CEL occurrence.
        assert_eq!(refs.len(), 2, "set + read: {refs:?}");
    }

    #[test]
    fn definition_off_symbol_is_none() {
        let doc = parsed("## Shot 1.\n:line[narrator]: just prose.\n");
        let off = 20; // in the prose
        assert!(definition_at(&doc, &load_core_snapshot(), off).is_none());
    }
}

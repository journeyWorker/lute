use serde::{Deserialize, Serialize};

/// Precomputed line-start table for byte <-> (line, col, utf16) mapping.
pub struct TextIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>, // byte offset of each line start
    line_utf16: Vec<u32>,    // file-relative UTF-16 offset of each line start
}

/// Where a byte offset sits: every CLI surface prints `line:column`; the LSP
/// speaks `utf16_col`.
#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub line: u32,      // 1-based
    pub column: u32,    // 1-based CHARACTER (Unicode scalar) column within line
    pub utf16_col: u32, // 0-based UTF-16 column within line
}

impl<'a> TextIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        let mut line_utf16 = vec![0u32];
        let mut units = 0u32;
        for (i, c) in text.char_indices() {
            units += c.len_utf16() as u32;
            if c == '\n' {
                line_starts.push(i + 1);
                line_utf16.push(units);
            }
        }
        Self {
            text,
            line_starts,
            line_utf16,
        }
    }

    /// The source text this index was built over. The byte offsets every `Span`
    /// carries index into exactly this string.
    pub fn text(&self) -> &'a str {
        self.text
    }

    fn line_of(&self, byte: usize) -> usize {
        match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    pub fn position(&self, byte: usize) -> Position {
        let line_ix = self.line_of(byte);
        let line_start = self.line_starts[line_ix];
        let slice = &self.text[line_start..byte];
        let (chars, utf16_col) = slice
            .chars()
            .fold((0u32, 0u32), |(n, u), c| (n + 1, u + c.len_utf16() as u32));
        Position {
            line: line_ix as u32 + 1,
            column: chars + 1,
            utf16_col,
        }
    }

    /// File-relative UTF-16 units before `byte`: the line's precomputed start
    /// plus the units within the line, so a span costs O(line), not O(file).
    fn utf16_offset(&self, byte: usize) -> u32 {
        let line_ix = self.line_of(byte);
        let line_start = self.line_starts[line_ix];
        self.line_utf16[line_ix]
            + self.text[line_start..byte]
                .chars()
                .map(|c| c.len_utf16() as u32)
                .sum::<u32>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: u32,               // 1-based, of byte_start
    pub column: u32,             // 1-based character column of byte_start
    pub utf16_range: (u32, u32), // file-relative UTF-16 offsets
}

impl Span {
    pub fn from_bytes(idx: &TextIndex, start: usize, end: usize) -> Self {
        let p = idx.position(start);
        Span {
            byte_start: start,
            byte_end: end,
            line: p.line,
            column: p.column,
            utf16_range: (idx.utf16_offset(start), idx.utf16_offset(end)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Content,
    Staging,
    Logic,
    Cel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evidence {
    Proven,
    Witnessed,
    Bounded { scope: String },
    Heuristic,
    Unknown,
}
impl Evidence {
    /// Construct a bounded verdict; empty scopes are rejected.
    pub fn bounded(scope: impl Into<String>) -> Option<Self> {
        let scope = scope.into();
        (!scope.trim().is_empty()).then_some(Self::Bounded { scope })
    }
}


impl Serialize for Evidence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let value = match self {
            Self::Proven => "proven",
            Self::Witnessed => "witnessed",
            Self::Bounded { .. } => "bounded",
            Self::Heuristic => "heuristic",
            Self::Unknown => "unknown",
        };
        s.serialize_str(value)
    }
}

impl<'de> Deserialize<'de> for Evidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        match value.as_str() {
            "proven" => Ok(Self::Proven),
            "witnessed" => Ok(Self::Witnessed),
            "bounded" => Err(serde::de::Error::custom(
                "bounded evidence requires a non-empty scope",
            )),
            "heuristic" => Ok(Self::Heuristic),
            "unknown" => Ok(Self::Unknown),
            _ => Err(serde::de::Error::custom("invalid evidence")),
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixit {
    pub title: String,
    pub kind: String, // e.g. "quickfix"
    pub edit: Vec<TextEdit>,
    pub confidence: u8, // 0..=100
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub span: Span,
    pub new_text: String,
}

/// One diagnostic. `message` is written by the producer and may cite the
/// spec (`(dsl 0.24.0 §4)`); every output surface shows [`Self::text`], the
/// plain sentence, and names the spec through `lute --explain <CODE>` and the
/// JSON `spec` field instead (dsl 0.27.0 §9, T3-17).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String, // stable, e.g. "E-UNDECLARED"
    pub severity: Severity,
    pub message: String,
    /// Evidence attached to an analysis-derived diagnostic.
    pub evidence: Option<Evidence>,
    pub span: Span,
    pub layer: Layer,
    pub fixits: Vec<Fixit>,
    pub provenance: Option<String>,
    /// Further document-order occurrences of the SAME root-cause diagnostic
    /// (dsl 0.4.0 §8.2 C1/C5, D11): populated ONLY by `lute-check`'s
    /// `collapse_same_root` post-pass, which folds every repeat of a same-
    /// code/same-root-subject read into the FIRST occurrence's `covered`
    /// instead of emitting N identical diagnostics. Additive — never removed
    /// or retyped; empty (and un-serialized) for every diagnostic that is not
    /// a collapse primary.
    pub covered: Vec<Span>,
    /// Sub-diagnostics surfaced from ANOTHER file, attributed to it (dsl
    /// 0.5.0 §2.2 importer-visible component sub-diagnostics): populated on
    /// an `E-COMPONENT-PARSE` failure with the failed component's OWN
    /// parse/frontmatter diagnostics, so an importing document's output
    /// (human or `--json`) carries what actually failed in the imported file
    /// without a separate re-`check` of it. Empty for every other diagnostic.
    pub related: Vec<RelatedDiagnostic>,
}

#[derive(Deserialize)]
struct DiagnosticWire {
    code: String,
    severity: Severity,
    message: String,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    span: Span,
    layer: Layer,
    #[serde(default)]
    fixits: Vec<Fixit>,
    #[serde(default)]
    provenance: Option<String>,
    #[serde(default)]
    covered: Vec<Span>,
    #[serde(default)]
    related: Vec<RelatedDiagnostic>,
}

impl<'de> Deserialize<'de> for Diagnostic {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = DiagnosticWire::deserialize(d)?;
        let evidence = match wire.evidence.as_deref() {
            None => {
                if wire.scope.is_some() {
                    return Err(serde::de::Error::custom("scope requires bounded evidence"));
                }
                None
            }
            Some("bounded") => {
                let scope = wire.scope.ok_or_else(|| {
                    serde::de::Error::custom("bounded evidence requires a non-empty scope")
                })?;
                Some(Evidence::bounded(scope).ok_or_else(|| {
                    serde::de::Error::custom("bounded evidence requires a non-empty scope")
                })?)
            }
            Some(value) => {
                if wire.scope.is_some() {
                    return Err(serde::de::Error::custom(
                        "scope is only valid for bounded evidence",
                    ));
                }
                Some(match value {
                    "proven" => Evidence::Proven,
                    "witnessed" => Evidence::Witnessed,
                    "heuristic" => Evidence::Heuristic,
                    "unknown" => Evidence::Unknown,
                    _ => return Err(serde::de::Error::custom("invalid evidence")),
                })
            }
        };
        Ok(Self {
            code: wire.code,
            severity: wire.severity,
            message: wire.message,
            evidence,
            span: wire.span,
            layer: wire.layer,
            fixits: wire.fixits,
            provenance: wire.provenance,
            covered: wire.covered,
            related: wire.related,
        })
    }
}

impl Diagnostic {
    /// The message as an author reads it ([`plain_message`]), with analysis
    /// evidence made explicit where it is not a proof or direct witness.
    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        let message = plain_message(&self.message);
        let suffix = match &self.evidence {
            Some(Evidence::Bounded { scope }) => Some(format!(" [bounded: {scope}]")),
            Some(Evidence::Heuristic) => Some(" [heuristic]".to_string()),
            Some(Evidence::Unknown) => Some(" [unknown]".to_string()),
            _ => None,
        };
        match suffix {
            Some(suffix) => std::borrow::Cow::Owned(format!("{message}{suffix}")),
            None => message,
        }
    }
}

/// JSON as every surface prints it: `message` is the plain sentence
/// ([`Diagnostic::text`]) and the spec sections it cited move to `spec`
/// (omitted when none) — field order and every other field unchanged.
impl Serialize for Diagnostic {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let spec = spec_citations(&self.message);
        let mut st = s.serialize_struct("Diagnostic", 12)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("severity", &self.severity)?;
        st.serialize_field("message", &plain_message(&self.message))?;
        st.serialize_field("span", &self.span)?;
        st.serialize_field("layer", &self.layer)?;
        if let Some(evidence) = &self.evidence {
            st.serialize_field("evidence", evidence)?;
            if let Evidence::Bounded { scope } = evidence {
                st.serialize_field("scope", scope)?;
            }
        }
        if !self.fixits.is_empty() {
            st.serialize_field("fixits", &self.fixits)?;
        }
        if let Some(p) = &self.provenance {
            st.serialize_field("provenance", p)?;
        }
        if !self.covered.is_empty() {
            st.serialize_field("covered", &self.covered)?;
        }
        if !self.related.is_empty() {
            st.serialize_field("related", &self.related)?;
        }
        if !spec.is_empty() {
            st.serialize_field("spec", &spec)?;
        }
        st.end()
    }
}

/// The website's diagnostics reference: one `### <CODE>` section per code,
/// rendered from the registry in `crates/lute-cli/src/codes.rs`.
pub const DIAGNOSTICS_REFERENCE: &str = "https://lute-lang.vercel.app/reference/diagnostics/";

/// The reference anchor of `code` (`…/reference/diagnostics/#e-set-shape`).
/// Every `E-`/`W-` code has one — the registry's drift guard fails on an
/// emitted code it lacks; a lint rule's `L-` code (named by its author) has
/// none.
pub fn doc_url(code: &str) -> Option<String> {
    (code.starts_with("E-") || code.starts_with("W-"))
        .then(|| format!("{DIAGNOSTICS_REFERENCE}#{}", code.to_ascii_lowercase()))
}

/// `message` without its spec citations (dsl 0.27.0 §9, T3-17): a
/// parenthetical whose every comma/semicolon-separated part cites the spec —
/// `(dsl 0.24.0 §4)`, `(dsl 0.3.0 §4, D4)`, `(dsl 0.24 T3-8)` — is removed
/// with the space before it; in a mixed one (`(expected one of …, dsl 0.3.0
/// §4)`) only the citing parts go. Parentheses inside backticks are code and
/// never touched. Borrowed when there is nothing to remove.
pub fn plain_message(message: &str) -> std::borrow::Cow<'_, str> {
    let groups = citation_groups(message);
    if groups.is_empty() {
        return std::borrow::Cow::Borrowed(message);
    }
    let mut out = String::with_capacity(message.len());
    let mut at = 0;
    for g in &groups {
        let mut start = g.open;
        if g.kept.is_empty() && message[..start].ends_with(' ') {
            start -= 1;
        }
        out.push_str(&message[at..start]);
        if !g.kept.is_empty() {
            out.push('(');
            out.push_str(&g.kept.join(", "));
            out.push(')');
        }
        at = g.close + 1;
    }
    out.push_str(&message[at..]);
    std::borrow::Cow::Owned(out.trim_end().to_string())
}

/// The spec sections `message` cites, in order (`["dsl 0.24.0 §4"]`) — what
/// [`plain_message`] removes.
pub fn spec_citations(message: &str) -> Vec<String> {
    citation_groups(message)
        .into_iter()
        .flat_map(|g| g.cited)
        .collect()
}

/// One parenthetical of a message holding at least one citation: its byte
/// range (`open` is the `(`, `close` the `)`), the parts it keeps and the
/// parts that cite the spec.
struct CitationGroup {
    open: usize,
    close: usize,
    kept: Vec<String>,
    cited: Vec<String>,
}

fn citation_groups(message: &str) -> Vec<CitationGroup> {
    let mut out = Vec::new();
    if !message.contains('§') && !message.contains("dsl ") && !message.contains("Appendix ") {
        return out;
    }
    let b = message.as_bytes();
    let (mut i, mut code) = (0, false);
    while i < b.len() {
        match b[i] {
            b'`' => code = !code,
            b'(' if !code => {
                // The matching `)`, skipping nested groups and code spans.
                let (mut depth, mut j, mut inner_code) = (0usize, i, false);
                let close = loop {
                    if j >= b.len() {
                        break None;
                    }
                    match b[j] {
                        b'`' => inner_code = !inner_code,
                        b'(' if !inner_code => depth += 1,
                        b')' if !inner_code => {
                            depth -= 1;
                            if depth == 0 {
                                break Some(j);
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                };
                let Some(close) = close else {
                    break;
                };
                let (kept, cited): (Vec<String>, Vec<String>) = split_parts(&message[i + 1..close])
                    .into_iter()
                    .partition(|p| !is_citation(p));
                if !cited.is_empty() {
                    out.push(CitationGroup {
                        open: i,
                        close,
                        kept,
                        cited,
                    });
                }
                i = close;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// `inner` split at its top-level `,` / `;` (outside nested parentheses and
/// code spans), each part trimmed.
fn split_parts(inner: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut code, mut from) = (0usize, false, 0);
    for (i, c) in inner.char_indices() {
        match c {
            '`' => code = !code,
            '(' if !code => depth += 1,
            ')' if !code => depth = depth.saturating_sub(1),
            ',' | ';' if !code && depth == 0 => {
                parts.push(inner[from..i].trim().to_string());
                from = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[from..].trim().to_string());
    parts
}

/// Whether one parenthetical part cites the spec: `dsl 0.24.0 §4`,
/// `0.26.0 §2.3`, `§7.6`, `dsl 0.24 T3-8`, `dsl 0.24.0`, a decision `D4` /
/// `D-L` (bare or versioned, `dsl 0.9.0 D-C`), a prerelease finding
/// `prerelease N8`, an appendix `Appendix C`, a dated design
/// (`dsl 2026-08-31 §4`, `subquest design 2026-08-31 §4`) or another spec
/// document's section (`plugin §7`).
fn is_citation(part: &str) -> bool {
    let p = part.trim();
    let (rest, named) = ["dsl ", "spec ", "plugin ", "subquest design "]
        .iter()
        .find_map(|prefix| p.strip_prefix(prefix))
        .map(|rest| (rest.trim_start(), true))
        .unwrap_or((p, false));
    if rest.starts_with('§') || rest.starts_with("prerelease N") || is_appendix(rest) {
        return true;
    }
    if is_decision(rest) {
        return true;
    }
    let version_len = if is_date(rest) {
        "YYYY-MM-DD".len()
    } else {
        rest.find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len())
    };
    let version = &rest[..version_len];
    if version.is_empty()
        || !(version.contains('.') || version.contains('-'))
        || !version.starts_with(|c: char| c.is_ascii_digit())
    {
        return false;
    }
    let after = rest[version_len..].trim_start();
    (named && after.is_empty())
        || after.starts_with('§')
        || is_appendix(after)
        || is_decision(after)
        || (after.starts_with('T')
            && after[1..].starts_with(|c: char| c.is_ascii_digit())
            && after.contains('-'))
}

/// A dated design document's version: `2026-08-31`, at the start of `s`.
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[..10].iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
        && !b.get(10).is_some_and(u8::is_ascii_alphanumeric)
}

/// An appendix reference: `Appendix C`, `Appendix C1`.
fn is_appendix(s: &str) -> bool {
    s.strip_prefix("Appendix ").is_some_and(|rest| {
        let mut cs = rest.chars();
        cs.next().is_some_and(|c| c.is_ascii_uppercase())
            && cs.all(|c| c.is_ascii_digit() || c == '.')
    })
}

/// A design-decision reference: `D4`, `D12`, `D-L`, `D-C`.
fn is_decision(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('D') else {
        return false;
    };
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    !rest.is_empty()
        && rest.len() <= 3
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
}

/// One diagnostic imported from another file (dsl 0.5.0 §2.2), attributed to
/// it: `file` names the file `diagnostic.span` is relative to (NOT the
/// document the outer [`Diagnostic`] belongs to).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedDiagnostic {
    pub file: String,
    pub diagnostic: Diagnostic,
}

/// Stable node id: assigned once, survives edits (dsl §12 `lineId` principle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableId(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_index_maps_byte_to_line_col_and_utf16() {
        // "a\nsé" : 'é' is 2 bytes (U+00E9), 1 UTF-16 unit
        let idx = TextIndex::new("a\nsé");
        // byte 0 = line 1 col 1
        let p0 = idx.position(0);
        assert_eq!((p0.line, p0.column), (1, 1));
        // byte 2 = start of line 2 ('s')
        let p2 = idx.position(2);
        assert_eq!((p2.line, p2.column), (2, 1));
        // 'é' begins at byte 3; its UTF-16 column within line 2 is 1 (0-based), char column 2
        let p3 = idx.position(3);
        assert_eq!((p3.line, p3.column), (2, 2));
        assert_eq!(p3.utf16_col, 1);
    }

    /// The column counts characters, not bytes: Korean (3 bytes, 1 UTF-16
    /// unit each) and an astral emoji (4 bytes, 2 units) before the span.
    #[test]
    fn column_counts_characters_after_a_multibyte_prefix() {
        let text = "x\n@수아: 안녕 😀 {{run.x}}\n";
        let idx = TextIndex::new(text);
        let at = text.find("{{").unwrap();
        let s = Span::from_bytes(&idx, at, at + 2);
        assert_eq!(s.line, 2);
        assert_eq!(s.column, 11, "`@수아: 안녕 😀 ` is 10 characters");
        assert_eq!(
            idx.position(at).utf16_col,
            11,
            "the emoji is two UTF-16 units"
        );
        assert_eq!(s.utf16_range, (13, 15));
    }

    #[test]
    fn span_from_bytes_fills_both_encodings() {
        let idx = TextIndex::new("hello");
        let s = Span::from_bytes(&idx, 1, 4);
        assert_eq!((s.byte_start, s.byte_end), (1, 4));
        assert_eq!(s.line, 1);
        assert_eq!(s.column, 2); // 1-based character column
        assert_eq!(s.utf16_range, (1, 4));
    }

    /// T3-17: a spec citation leaves the sentence an author reads; the
    /// sentence around it, code spans and ordinary parentheticals stay.
    #[test]
    fn plain_message_drops_spec_citations_only() {
        let cases = [
            (
                "entity kind `x` must declare one of `members:`/`open:` (dsl 0.3.0 §3.1, 0.26.0 §2.3)",
                "entity kind `x` must declare one of `members:`/`open:`",
            ),
            (
                "a component body must be presentational (dsl 0.4 §6.2): `::use` of `x` writes state",
                "a component body must be presentational: `::use` of `x` writes state",
            ),
            (
                "relation `r` has unknown `tier: t` (expected one of scene/run/user, dsl 0.3.0 §4)",
                "relation `r` has unknown `tier: t` (expected one of scene/run/user)",
            ),
            (
                "`into=\"x\"` is not declared (dsl 0.6.0 §2.2); an undeclared `into` cannot create a field",
                "`into=\"x\"` is not declared; an undeclared `into` cannot create a field",
            ),
            ("relation `r` field `k` is malformed (dsl 0.3.0 §4, D4)", "relation `r` field `k` is malformed"),
            ("rename the relation (dsl 0.24 T3-8)", "rename the relation"),
            ("before using `action` (dsl 0.9.0 D-C)", "before using `action`"),
            (
                "write `holds(owned(occasion.target))` (a fact) (dsl §7.6)",
                "write `holds(owned(occasion.target))` (a fact)",
            ),
            (
                "malformed fact pattern: expected `,` or `)` (dsl 0.3.0 §5, Appendix C)",
                "malformed fact pattern: expected `,` or `)`",
            ),
            (
                "the child can never complete (dsl 2026-08-31 §2.1): `quest.c.state` is never set",
                "the child can never complete: `quest.c.state` is never set",
            ),
            (
                "names its own quest (subquest design 2026-08-31 §4)",
                "names its own quest",
            ),
            (
                "segment admits only `enum` or `number` (plugin §7)",
                "segment admits only `enum` or `number`",
            ),
            ("a draft from 2026-08-31 (2026-08-31)", "a draft from 2026-08-31 (2026-08-31)"),
            ("the `(` is never closed (since 0.24.0)", "the `(` is never closed (since 0.24.0)"),
        ];
        for (message, plain) in cases {
            assert_eq!(plain_message(message), plain, "{message}");
        }
        assert_eq!(
            spec_citations("x (dsl 0.3.0 §3.1, 0.26.0 §2.3) y (dsl 0.24 T3-8)"),
            ["dsl 0.3.0 §3.1", "0.26.0 §2.3", "dsl 0.24 T3-8"]
        );
        assert!(matches!(
            plain_message("no citation"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// JSON carries the plain message and the citations as `spec`.
    #[test]
    fn serialized_diagnostic_moves_the_citation_to_spec() {
        let d = Diagnostic {
            code: "E-SET-SHAPE".into(),
            severity: Severity::Error,
            message: "bad (dsl 0.27.0 §7)".into(),
            span: Span::from_bytes(&TextIndex::new("ab"), 0, 1),
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
            evidence: None,
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["message"], "bad");
        assert_eq!(v["spec"], serde_json::json!(["dsl 0.27.0 §7"]));
        assert_eq!(
            doc_url("E-SET-SHAPE"),
            Some(format!("{DIAGNOSTICS_REFERENCE}#e-set-shape"))
        );
        assert_eq!(
            doc_url("L-SHORT-LINES"),
            None,
            "a lint rule's code has no section"
        );
    }
    #[test]
    fn evidence_suffix_and_json_fields() {
        let d = Diagnostic {
            code: "W-OBJECTIVE-STRANDED".into(),
            severity: Severity::Warning,
            message: "may be stranded".into(),
            span: Span::from_bytes(&TextIndex::new("x"), 0, 1),
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
            evidence: Some(Evidence::Bounded {
                scope: "declared clock windows; no path search".into(),
            }),
        };
        assert_eq!(
            d.text(),
            "may be stranded [bounded: declared clock windows; no path search]"
        );
        let value = serde_json::to_value(&d).unwrap();
        assert_eq!(value["evidence"], "bounded");
        assert_eq!(value["scope"], "declared clock windows; no path search");
    }

    #[test]
    fn bounded_diagnostic_round_trips_and_requires_scope() {
        let diagnostic = Diagnostic {
            code: "W-OBJECTIVE-STRANDED".into(),
            severity: Severity::Warning,
            message: "may be stranded".into(),
            span: Span::from_bytes(&TextIndex::new("x"), 0, 1),
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
            evidence: Some(Evidence::bounded("declared clock windows; no path search").unwrap()),
        };
        let json = serde_json::to_value(&diagnostic).unwrap();
        let restored: Diagnostic = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored, diagnostic);
        let missing = serde_json::json!({
            "code": "W-OBJECTIVE-STRANDED", "severity": "warning", "message": "x",
            "evidence": "bounded", "span": diagnostic.span, "layer": "logic"
        });
        assert!(serde_json::from_value::<Diagnostic>(missing).is_err());
        let empty = serde_json::json!({
            "code": "W-OBJECTIVE-STRANDED", "severity": "warning", "message": "x",
            "evidence": "bounded", "scope": "", "span": diagnostic.span, "layer": "logic"
        });
        assert!(serde_json::from_value::<Diagnostic>(empty).is_err());
        let whitespace = serde_json::json!({
            "code": "W-OBJECTIVE-STRANDED", "severity": "warning", "message": "x",
            "evidence": "bounded", "scope": "   ", "span": diagnostic.span, "layer": "logic"
        });
        assert!(serde_json::from_value::<Diagnostic>(whitespace).is_err());
    }
}

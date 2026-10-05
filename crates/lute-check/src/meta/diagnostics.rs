use super::*;
use super::diagnostic_helpers::yaml_key;
pub use super::diagnostic_helpers::{ident_from_name, infer_meta_kind_from_shape};
pub(crate) use super::diagnostic_helpers::yaml_shape;
/// `E-STATE-DECL`'s whole message (#21, T10.2): what is wrong with this
/// declaration and what the legal shape is, written from the YAML the author
/// typed rather than from `serde_yaml`'s report of how it failed to
/// deserialize into `StateDeclRaw`.
///
/// The library error cannot be forwarded and cannot be replaced by one fixed
/// sentence either. It says "invalid type: unit variant, expected newtype
/// variant" for BOTH the nesting mistake (`{ type: enum, values: [...] }`) and
/// a bare `type: enum`; "missing field `type`" for a declaration without one;
/// "unknown variant `nonsense`" followed by the entire internal `Type` union
/// for a bad name; and "expected struct StateDeclRaw" — a Rust type name —
/// for a non-mapping. One sentence blaming `values:` would be FALSE for four
/// of those five, which is worse than the serde text it replaces. So the
/// shape is classified here, from the value the author wrote.
///
/// Each arm returns the message WHOLE, as a single `format!` literal, because
/// `scripts/check-doc-snippets.py` pins every quoted diagnostic against the
/// scraped literals in `crates/*/src` and measures that they still reproduce
/// real output. A message assembled from shared `const` fragments resolves to
/// no literal and drops that floor.
pub(super) fn state_decl_message(path: &str, decl: &serde_yaml::Value) -> String {
    let Some(map) = decl.as_mapping() else {
        return format!(
            "invalid state declaration for `{path}`: a declaration is a mapping with a \
             `type:` key — `{{ type: int, default: 0 }}` — but this is {} \
             (dsl 0.8.0 §4)",
            yaml_shape(decl)
        );
    };
    if let Some(owner) = map.get(yaml_key("owner")) {
        if owner.as_str() != Some("engine") {
            return format!(
                "invalid state declaration for `{path}`: `owner:` is {}, but the only owner a \
                 declaration can name is `engine` (`owner: engine` — the engine writes the \
                 path; content may only read it); omit `owner:` for content-written state \
                 (dsl 0.22.0 §1.2)",
                owner
                    .as_str()
                    .map_or_else(|| yaml_shape(owner).to_string(), |s| format!("`{s}`"))
            );
        }
    }
    let Some(ty) = map.get(yaml_key("type")) else {
        return format!(
            "invalid state declaration for `{path}`: the declaration has no `type:` key; \
             author state is scalar, as `{{ type: int, default: 0 }}`, and an `enum` \
             NESTS its members inside `type:`, as `{{ type: {{ enum: [...] }} }}` \
             (dsl 0.8.0 §4)"
        );
    };
    let Some(name) = ty.as_str() else {
        return format!(
            "invalid state declaration for `{path}`: the `type:` value is malformed; author \
             state is scalar, as `{{ type: int, default: 0 }}`, and an `enum` NESTS its \
             members inside `type:`, as `{{ type: {{ enum: [...] }} }}` \
             (dsl 0.8.0 §4)"
        );
    };
    // `type: enum` is the one scalar type that is INCOMPLETE as a bare name —
    // its members nest one level inside `type:`. Hoisting them to a sibling
    // key is the four-word mistake copied straight out of `state-model.md`,
    // and that one nesting level is the whole of what this diagnostic exists
    // to say.
    if name == "enum" {
        return match ["values", "members", "options"]
            .into_iter()
            .find(|k| map.contains_key(yaml_key(k)))
        {
            Some(k) => format!(
                "invalid state declaration for `{path}`: `type: enum` is not complete on its \
                 own, and a top-level `{k}:` is not a declaration key — an `enum` NESTS its \
                 members inside `type:`, as `{{ type: {{ enum: [...] }} }}` (dsl 0.8.0 §4)"
            ),
            None => format!(
                "invalid state declaration for `{path}`: `type: enum` declares no members — \
                 an `enum` NESTS them inside `type:`, as \
                 `{{ type: {{ enum: [teen, adult] }}, default: teen }}` (dsl 0.8.0 §4)"
            ),
        };
    }
    // `list`/`record`/`map` name real types, so "unknown type" would be a lie.
    // They are incomplete as bare names AND barred from author state anyway;
    // the well-formed spelling lands on `E-STATE-COLLECTION`, whose remedy
    // this arm repeats verbatim so the two read as one rule.
    if matches!(name, "list" | "record" | "map") {
        return format!(
            "invalid state declaration for `{path}`: `type: {name}` is an incomplete \
             collection type, and author state cannot declare a collection type in any \
             case; it is scalar (int|double|bool|string|enum) — model collections as \
             `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot"
        );
    }
    if matches!(name, "bool" | "int" | "double" | "string") {
        return format!(
            "invalid state declaration for `{path}`: `type: {name}` is a valid type, so the \
             rest of the declaration is what is malformed; a declaration is \
             `{{ type: {name}, default: ... }}` and nothing else (dsl 0.8.0 §4)"
        );
    }
    if name == "number" {
        return format!(
            "invalid state declaration for `{path}`: `type: number` is no longer a valid \
             numeric type; use `type: int` for whole numbers or `type: double` for fractional \
             values (dsl 0.8.0 §4)"
        );
    }
    format!(
        "invalid state declaration for `{path}`: unknown type `{name}`; author state is \
         scalar — `int`, `double`, `bool`, `string`, or `enum`, and an `enum` NESTS its members \
         inside `type:`, as `{{ type: {{ enum: [...] }} }}` (dsl 0.8.0 §4)"
    )
}



pub fn meta_key_span(meta: &Meta, needle: &str) -> Span {
    // `raw_yaml` is usually the frontmatter interior sliced verbatim after the
    // 4-byte `"---\n"` opener (itself included in `meta.span`), so a `raw_yaml`
    // offset maps to the document by adding `meta.span.byte_start + 4`.
    //
    // A bare `.yaml`/`.yml` state schema (data-catalog foundation B2) has NO
    // envelope: `schema_import::read_and_parse` and `lute check <schema.yaml>`
    // both wrap the whole file in a synthetic `Meta` whose `raw_yaml` IS the
    // span. Adding a delimiter that is not there shifted every key span four
    // bytes right — a wrong column, and on a short first line a wrong LINE —
    // so "anchored at the offending key" was only ever true for `.lute` (#21,
    // T10.2). The two cases are told apart exactly: a real frontmatter span
    // covers at least `"---\n"` + `"---"` more bytes than its interior.
    const OPENER_LEN: usize = 4; // "---\n"

    // dsl 0.28.0 §4: keys a project `chapters:` chain derived sit below the
    // marker and have no text in the file; they are anchored at the scene's
    // `id:` — never at some other occurrence of the key's name (T3-18).
    if needle != "id" && crate::chapters::derived(meta, needle) {
        return meta_key_span(meta, "id");
    }
    let authored = crate::chapters::authored_yaml(&meta.raw_yaml);
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != authored.len();
    let base = meta.span.byte_start + if enveloped { OPENER_LEN } else { 0 };
    let at = |start: usize| Span {
        byte_start: start,
        byte_end: start + needle.len(),
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    // Key-aware scan: the needle at a line start (after indent), then `:`.
    let mut line_start = 0usize;
    for line in authored.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = line.trim_start().strip_prefix(needle) {
            if rest.trim_start().starts_with(':') {
                return at(base + line_start + indent);
            }
        }
        line_start += line.len();
    }
    // Fallbacks: naive first occurrence, then the whole frontmatter block.
    match authored.find(needle) {
        Some(idx) => at(base + idx),
        None if needle != "id" && meta.raw_yaml.len() > authored.len() => meta_key_span(meta, "id"),
        None => meta.span,
    }
}

/// The span of the mapping key at `path` (from the frontmatter's top level
/// down, e.g. `["state", "run.n", "defualt"]`) — a nested key, block or
/// one-line flow mapping, located by [`lute_manifest::yaml_text::key_span`].
/// Falls back to the deepest ancestor key found, then [`meta_key_span`] of
/// the first segment.
pub fn meta_path_span(meta: &Meta, path: &[&str]) -> Span {
    let authored = crate::chapters::authored_yaml(&meta.raw_yaml);
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != authored.len();
    let base = meta.span.byte_start + if enveloped { 4 } else { 0 };
    for depth in (1..=path.len()).rev() {
        if let Some(r) = lute_manifest::yaml_text::key_span(authored, &path[..depth]) {
            return Span {
                byte_start: base + r.start,
                byte_end: base + r.end,
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            };
        }
    }
    path.first()
        .map_or(meta.span, |first| meta_key_span(meta, first))
}

/// `E-META-PARSE` for a frontmatter serde_yaml rejects: the message and the
/// span of the offending character in the FILE (round-5 T3-2). serde_yaml
/// counts lines from the first line after the `---` opener, so its own
/// `at line N column M` was one line short and the diagnostic sat at `1:1`.
/// The anchor now carries the position, so the problem mark is dropped from
/// the message and any other mark (a context's) is renumbered to file lines.
/// The fix and its anchor come from [`lute_manifest::yaml_text::yaml_fault`].
pub(super) fn yaml_parse_error(meta: &Meta, e: &serde_yaml::Error) -> (String, Span) {
    // [`meta_key_span`]'s envelope rule: a `.lute` frontmatter's interior
    // starts after the 4-byte `"---\n"` opener; a bare `.yaml` has none.
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
    let base = meta.span.byte_start + if enveloped { 4 } else { 0 };
    let line_offset = meta.span.line.max(1) as usize - usize::from(!enveloped);
    let loc = e.location();
    let mut problem = String::new();
    let mut rest = e.to_string();
    while let Some(i) = rest.find(" at line ") {
        problem.push_str(&rest[..i]);
        let tail = &rest[i + " at line ".len()..];
        let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let ln = digits(tail);
        let Some(after_col) = tail[ln..].strip_prefix(" column ") else {
            problem.push_str(" at line ");
            rest = tail.to_string();
            continue;
        };
        let cn = digits(after_col);
        let (line, col) = (tail[..ln].parse().ok(), after_col[..cn].parse().ok());
        let is_problem = loc
            .as_ref()
            .is_some_and(|l| Some(l.line()) == line && Some(l.column()) == col);
        if !is_problem {
            if let (Some(line), Some(col)) = (line, col) {
                problem.push_str(&format!(" at line {} column {col}", line + line_offset));
            }
        }
        rest = after_col[cn..].to_string();
    }
    problem.push_str(&rest);

    // A `.lute` document's frontmatter, or a whole `.yaml` file (a schema).
    let what = if enveloped {
        "the frontmatter"
    } else {
        "this file"
    };
    if loc.is_none() {
        return (
            format!("{what} does not parse as YAML: {problem}"),
            meta.span,
        );
    }
    // The shared YAML fault reader words the known slips (a tab in the
    // indentation, a quote nested or never closed, `key:value`, a value
    // holding `: `, …) with their fix, and anchors at the slip; anything
    // else is the library's sentence, without its line/column marks.
    let fault = lute_manifest::yaml_text::yaml_fault(&meta.raw_yaml, e);
    let message = match fault.message.strip_prefix("the YAML does not parse: ") {
        Some(problem) => format!("{what} does not parse as YAML: {problem}"),
        None => format!("{what} does not parse as YAML — {}", fault.message),
    };
    let mut start = fault.offset.min(meta.raw_yaml.len());
    while !meta.raw_yaml.is_char_boundary(start) {
        start -= 1;
    }
    let len = meta.raw_yaml[start..]
        .chars()
        .next()
        .map_or(0, char::len_utf8);
    let at = Span {
        byte_start: base + start,
        byte_end: base + start + len,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    (message, at)
}

/// Authoritative same-block duplicate-key scan for `relations:`/`entities:`
/// (dsl 0.3.0 T5): `serde_yaml::Mapping` silently collapses a repeated
/// mapping key before [`lute_manifest::relations::parse_relations`]/
/// `parse_entity_kinds` ever see it, so their own `dups` field is
/// best-effort. This is a dumb, total line scan over the RAW frontmatter
/// text (never a YAML re-parse): find the top-level `<block_key>:` line,
/// then collect every direct child key at the FIRST indent level seen under
/// it (a deeper-nested key, e.g. `members:`/`args:` inside a block-style
/// entry, is ignored — only entries at the entry-list's own indent count).
/// A name repeated at that level is recorded once, at its second
/// occurrence.
pub(super) fn scan_block_dup_names(raw_yaml: &str, block_key: &str) -> Vec<String> {
    let prefix = format!("{block_key}:");
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    let mut dups = Vec::new();
    let mut in_block = false;
    let mut entry_indent: Option<usize> = None;
    for line in raw_yaml.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_block = trimmed.starts_with(&prefix);
            entry_indent = None;
            continue;
        }
        if !in_block || trimmed.is_empty() {
            continue;
        }
        let want_indent = *entry_indent.get_or_insert(indent);
        if indent != want_indent {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let name = trimmed[..colon].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let count = seen.entry(name.to_string()).or_insert(0);
        *count += 1;
        if *count == 2 {
            dups.push(name.to_string());
        }
    }
    dups
}

/// Recovery helper for [`parse_meta_kind`]'s initial whole-document YAML
/// parse (dsl 0.3.0 T5/T7): `serde_yaml` REJECTS a literal duplicate key
/// anywhere in the document (it does not silently collapse a repeat, contra
/// [`scan_block_dup_names`]'s original assumption), which would otherwise
/// take down the ENTIRE frontmatter lift over a same-block
/// `entities:`/`relations:` dup that this crate has a dedicated diagnostic
/// for (`E-KIND-NAME-CLASH`/`E-RELATION-DUP`). Returns a copy of `raw_yaml`
/// with every occurrence AFTER THE FIRST of a same-indent-level child key
/// under `entities:`/`relations:` commented out (indent preserved, a `#`
/// inserted) — just enough for the retry parse to succeed and lift every
/// OTHER field; mirrors `scan_block_dup_names`'s exact block/indent-tracking
/// so both agree on which key is "the" duplicate.
pub(super) fn sanitize_dup_block_keys(raw_yaml: &str) -> String {
    const BLOCKS: [&str; 2] = ["entities", "relations"];
    let mut seen: BTreeMap<(&str, String), u32> = BTreeMap::new();
    let mut in_block: Option<&str> = None;
    let mut entry_indent: Option<usize> = None;
    let mut out = String::with_capacity(raw_yaml.len());
    for line in raw_yaml.split_inclusive('\n') {
        let body_len = line.trim_end_matches(['\n', '\r']).len();
        let (body, nl) = line.split_at(body_len);
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if indent == 0 {
            in_block = BLOCKS
                .iter()
                .copied()
                .find(|k| trimmed.starts_with(&format!("{k}:")));
            entry_indent = None;
            out.push_str(line);
            continue;
        }
        let Some(block) = in_block else {
            out.push_str(line);
            continue;
        };
        if trimmed.is_empty() {
            out.push_str(line);
            continue;
        }
        let want_indent = *entry_indent.get_or_insert(indent);
        if indent != want_indent {
            out.push_str(line);
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            out.push_str(line);
            continue;
        };
        let name = trimmed[..colon].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            out.push_str(line);
            continue;
        }
        let count = seen.entry((block, name.to_string())).or_insert(0);
        *count += 1;
        if *count >= 2 {
            out.push_str(&body[..indent]);
            out.push('#');
            out.push_str(trimmed);
            out.push_str(nl);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// The keys a `state:` row may carry ([`StateDeclRaw`]).
const STATE_ROW_KEYS: [&str; 4] = ["type", "default", "owner", "per"];

/// dsl 0.28.0 §1 (T1-2): every key of a `state:` row outside
/// [`STATE_ROW_KEYS`], with its message. Two keys other layers spell get the
/// state row's own spelling: `reserved:` (a relation's engine-ownership) and
/// `tier:` (a quest's or relation's lifetime — a state path's tier is its
/// first segment).
pub(super) fn unknown_state_row_keys(path: &str, row: &serde_yaml::Value) -> Vec<(String, String)> {
    let Some(row) = row.as_mapping() else {
        return Vec::new();
    };
    row.keys()
        .filter_map(|k| k.as_str())
        .filter(|k| !STATE_ROW_KEYS.contains(k))
        .map(|key| {
            let message = match key {
                "reserved" => format!(
                    "`{path}`: a state row says who writes it with `owner: engine` — \
                     `reserved: true` is how a relation says it"
                ),
                "tier" => format!(
                    "`{path}`: a state path's tier is its first segment (`{}.`), not a \
                     `tier:` key — rename the path to change its tier",
                    map_prefix(path)
                ),
                _ => format!(
                    "`{path}`: unknown key `{key}`{} (a state row takes `type:`, `default:`, \
                     `owner:` and `per:`)",
                    lute_manifest::suggest::did_you_mean(key, STATE_ROW_KEYS)
                ),
            };
            (key.to_string(), message)
        })
        .collect()
}

/// Raw `state:` entry (dsl §9.3): `{ type, default?, owner?, per? }`. `Type`
/// reuses the manifest's manual serde (inline `{ enum: [...] }` etc. work).
#[derive(serde::Deserialize)]
pub(super) struct StateDeclRaw {
    #[serde(rename = "type")]
    pub(super) ty: Type,
    #[serde(default)]
    pub(super) default: Option<Literal>,
    #[serde(default)]
    pub(super) owner: Option<lute_manifest::types::Owner>,
    /// dsl 0.24.0 §3: the closed entity kind this path is indexed by.
    #[serde(default)]
    pub(super) per: Option<String>,
}

/// The members a `per: <kind>` state family is declared over (dsl 0.24.0
/// §3) — including the members its `subsetOf:` sub-kinds add (dsl 0.26.0
/// §2.3, closed by the one [`lute_manifest::relations::imply_sub_kind_members`]
/// every other kind consumer uses; dsl 0.27.0 §2) — or why it cannot be: the
/// kind is `open:` or malformed. `kinds` is this document's own `entities:`.
pub(super) fn per_members(
    kinds: &lute_manifest::relations::ParsedKinds,
    kind: &str,
) -> Result<Vec<String>, &'static str> {
    let mut closed = kinds.kinds.clone();
    lute_manifest::relations::imply_sub_kind_members(&mut closed, &kinds.order);
    closed_per_members(&closed, kind)
}

/// [`per_members`] over kinds whose sub-kind members are already implied.
pub(super) fn closed_per_members(
    kinds: &BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
    kind: &str,
) -> Result<Vec<String>, &'static str> {
    use lute_manifest::relations::KindShape;
    match kinds.get(kind).map(|k| &k.shape) {
        Some(KindShape::Members(ms)) => Ok(ms.clone()),
        Some(KindShape::Open) => {
            Err("names an `open:` entity kind, whose members the engine registers at runtime")
        }
        Some(KindShape::Invalid) => Err("names a malformed entity kind"),
        None => Err(
            "names no entity kind declared in this document's `entities:` or in a \
                     schema it imports",
        ),
    }
}

/// `E-STATE-DECL`'s message for a `per:` that cannot index `path`.
pub(super) fn per_fault(path: &str, kind: &str, why: &str) -> String {
    format!(
        "invalid state declaration for `{path}`: `per: {kind}` {why}; `per:` indexes a path by a \
         closed entity kind (`members: [...]`), declaring `{path}.<member>` for every member"
    )
}

/// A `per: <kind>` state family whose kind the declaring document does not
/// declare itself — another schema may (dsl 0.28.0: `per:` reads the merged
/// kinds, as `subsetOf:` and `add:` do). Expanded by [`expand_per_pending`]
/// once the imports are merged; `span` is the path's key in its document.
#[derive(Clone, Debug)]
pub struct PendingPer {
    pub path: String,
    pub kind: String,
    pub decl: StateDecl,
    pub default: Option<Literal>,
    pub span: Span,
}

/// The member decls and `path → kind` index entries each [`PendingPer`]
/// declares over `kinds` (sub-kind members already implied — the merged
/// vocabulary's), and an `E-STATE-DECL` at its key for one whose kind is
/// still unknown, `open:` or malformed, or whose `default:` map is wrong.
pub(crate) fn expand_per_pending(
    pending: &[PendingPer],
    kinds: &BTreeMap<String, lute_manifest::relations::EntityKindDecl>,
) -> (
    Vec<(String, StateDecl)>,
    Vec<(String, String)>,
    Vec<Diagnostic>,
) {
    let (mut decls, mut index, mut diags) = (Vec::new(), Vec::new(), Vec::new());
    for p in pending {
        match closed_per_members(kinds, &p.kind) {
            Ok(members) => {
                let defaults = per_member_defaults(
                    &p.path,
                    &p.kind,
                    &members,
                    &p.decl.ty,
                    p.default.clone(),
                    &mut diags,
                    p.span,
                );
                for (m, default) in members.iter().zip(defaults) {
                    decls.push((
                        format!("{}.{m}", p.path),
                        StateDecl {
                            default,
                            ..p.decl.clone()
                        },
                    ));
                }
                index.push((p.path.clone(), p.kind.clone()));
            }
            Err(why) => diags.push(state_decl_diag(per_fault(&p.path, &p.kind, why), p.span)),
        }
    }
    (decls, index, diags)
}

/// An `E-STATE-DECL` at a `state:` key.
pub(super) fn state_decl_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-STATE-DECL".to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

const MAX_SAFE_INT_DEFAULT: i64 = 1_i64 << 53;

fn int_default_in_range(ty: &Type, value: &Literal) -> bool {
    !matches!(
        (ty, value),
        (Type::Int, Literal::Int(n))
            if *n < -MAX_SAFE_INT_DEFAULT || *n > MAX_SAFE_INT_DEFAULT
    )
}
/// A scalar default must match the declared type exactly (`int` and `double`
/// are distinct); a list default is `E-STATE-DECL` and installs no default.
pub(super) fn scalar_default(
    path: &str,
    ty: &Type,
    default: Option<Literal>,
    diags: &mut Vec<Diagnostic>,
    span: Span,
) -> Option<Literal> {
    match default {
        Some(Literal::List(_)) => {
            diags.push(state_decl_diag(
                format!(
                    "invalid state declaration for `{path}`: `default:` is a list, but author \
                     state is scalar (int|double|bool|string|enum) — give one value (dsl 0.8.0 §4)"
                ),
                span,
            ));
            None
        }
        Some(value) if !type_accepts(ty, &value) => {
            diags.push(state_decl_diag(
                format!(
                    "invalid state declaration for `{path}`: `default:` gives {}, which is not \
                     a `{}` (dsl 0.8.0 §4)",
                    lit_str(&value),
                    type_str(ty)
                ),
                span,
            ));
            None
        }
        Some(Literal::Int(n))
            if matches!(ty, Type::Int) && !int_default_in_range(ty, &Literal::Int(n)) =>
        {
            diags.push(state_decl_diag(
                format!(
                    "invalid state declaration for `{path}`: `default: {n}` is outside the \
                     exact integer range ±2^53 (dsl 0.8.0 §4)"
                ),
                span,
            ));
            None
        }
        None => None,
        Some(value) => Some(value),
    }
}

/// The default of each member of a `per: <kind>` family (dsl 0.24.0 §3), in
/// `members` order. A scalar `default:` is every member's; a map gives
/// members their own — `{ isolde: 2, corvin: 0 }` — where every key is a
/// member or `_`, the fallback for the members it does not name. A member
/// the map neither names nor falls back for, a key that is no member, a
/// value that is not a scalar of the path's type, and a list default are
/// each `E-STATE-DECL`; the member they concern gets no default.
pub(super) fn per_member_defaults(
    path: &str,
    kind: &str,
    members: &[String],
    ty: &Type,
    default: Option<Literal>,
    diags: &mut Vec<Diagnostic>,
    span: Span,
) -> Vec<Option<Literal>> {
    let map = match default {
        Some(Literal::Map(map)) => map,
        other => {
            let d = scalar_default(path, ty, other, diags, span);
            return vec![d; members.len()];
        }
    };
    let mut bad = |msg: String| {
        diags.push(state_decl_diag(
            format!("invalid state declaration for `{path}`: {msg} (dsl 0.24.0 §3)"),
            span,
        ));
    };
    for key in map.keys().filter(|k| *k != "_" && !members.contains(k)) {
        let hint = lute_manifest::suggest::nearest(key, members.iter().map(String::as_str), 2)
            .map(|n| format!(" — did you mean `{n}`?"))
            .unwrap_or_default();
        bad(format!(
            "`default:` names `{key}`, which is not a member of entity kind `{kind}` [{}]{hint}",
            members.join(", ")
        ));
    }
    let mut valid = BTreeMap::new();
    for (key, value) in &map {
        if matches!(value, Literal::Map(_) | Literal::List(_)) || !type_accepts(ty, value) {
            bad(format!(
                "`default:` gives `{key}` the value {}, which is not a `{}`",
                lit_str(value),
                type_str(ty)
            ));
        } else if !int_default_in_range(ty, value) {
            bad(format!(
                "`default:` gives `{key}` an integer outside the exact range ±2^53"
            ));
        } else {
            valid.insert(key.as_str(), value);
        }
    }
    let fallback = map.contains_key("_");
    let missing: Vec<&str> = members
        .iter()
        .map(String::as_str)
        .filter(|m| !map.contains_key(*m))
        .collect();
    if !fallback && !missing.is_empty() {
        bad(format!(
            "`default:` gives no value for {} — name every member of `{kind}`, or add a fallback \
             for the rest with `_: <value>`",
            missing
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    members
        .iter()
        .map(|m| {
            valid
                .get(m.as_str())
                .or_else(|| valid.get("_"))
                .map(|v| (*v).clone())
        })
        .collect()
}


pub(super) fn map_prefix(path: &str) -> &str {
    path.split_once('.').map_or(path, |(head, _)| head)
}

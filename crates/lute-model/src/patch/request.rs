//! The patch request shape as one table (dsl 0.47.0 §5.2). The request parser
//! reads its op fields and preserve keys from here, and `lute schema patch`
//! describes the request from here, so the two cannot drift.

use lute_semantic::NodeKind;
use serde_json::{json, Value};

/// One field of the request, its `base`, or an edit op.
#[derive(Clone, Copy, Debug)]
pub struct RequestField {
    pub name: &'static str,
    pub required: bool,
    /// `string`, `nodeKey`, `path`, `sha256`, `span`, `object`,
    /// `map<path,sha256>`, or a `[]` list of one of them (`edit[]`,
    /// `preserve[]`).
    pub ty: &'static str,
}

/// One edit operation (`"op"`) and its fields.
#[derive(Clone, Copy, Debug)]
pub struct EditOp {
    pub op: &'static str,
    pub fields: &'static [RequestField],
}

/// One `preserve` item key and the type of its value.
#[derive(Clone, Copy, Debug)]
pub struct PreserveKey {
    pub key: &'static str,
    pub ty: &'static str,
}

const fn field(name: &'static str, required: bool, ty: &'static str) -> RequestField {
    RequestField { name, required, ty }
}

/// The request's top-level fields.
pub const REQUEST_FIELDS: &[RequestField] = &[
    field("base", true, "object"),
    field("targets", false, "nodeKey[]"),
    field("edits", false, "edit[]"),
    field("preserve", false, "preserve[]"),
];

/// The fields of `base`.
pub const BASE_FIELDS: &[RequestField] = &[
    field("project", true, "sha256"),
    field("files", false, "map<path,sha256>"),
];

const NODE_TEXT: &[RequestField] = &[
    field("node", true, "nodeKey"),
    field("text", true, "string"),
];

/// Every edit op, in the order the documentation lists them.
pub const EDIT_OPS: &[EditOp] = &[
    EditOp {
        op: "replaceNode",
        fields: NODE_TEXT,
    },
    EditOp {
        op: "insertBefore",
        fields: NODE_TEXT,
    },
    EditOp {
        op: "insertAfter",
        fields: NODE_TEXT,
    },
    EditOp {
        op: "replaceAttr",
        fields: &[
            field("node", true, "nodeKey"),
            field("attr", true, "string"),
            field("value", true, "string"),
        ],
    },
    EditOp {
        op: "removeNode",
        fields: &[field("node", true, "nodeKey")],
    },
    EditOp {
        op: "replaceText",
        fields: &[
            field("file", true, "path"),
            field("rev", true, "sha256"),
            field("span", true, "span"),
            field("text", true, "string"),
        ],
    },
    EditOp {
        op: "createFile",
        fields: &[field("path", true, "path"), field("text", true, "string")],
    },
    EditOp {
        op: "moveFile",
        fields: &[field("from", true, "path"), field("to", true, "path")],
    },
];

/// Every `preserve` key. `true` means the value must be the literal `true`.
pub const PRESERVE_KEYS: &[PreserveKey] = &[
    PreserveKey {
        key: "ids",
        ty: "nodeKey[]",
    },
    PreserveKey {
        key: "lineIds",
        ty: "string[]",
    },
    PreserveKey {
        key: "voiceKeys",
        ty: "string[]",
    },
    PreserveKey {
        key: "choiceEffects",
        ty: "nodeKey[]",
    },
    PreserveKey {
        key: "rewards",
        ty: "nodeKey[]",
    },
    PreserveKey {
        key: "hostContracts",
        ty: "true",
    },
    PreserveKey {
        key: "conditions",
        ty: "nodeKey[]",
    },
    PreserveKey {
        key: "reachability",
        ty: "nodeKey[]",
    },
    PreserveKey {
        key: "constraints",
        ty: "true",
    },
];

/// Every node-key kind (`kind:key`).
pub const NODE_KINDS: &[NodeKind] = &[
    NodeKind::Project,
    NodeKind::Document,
    NodeKind::Scene,
    NodeKind::Beat,
    NodeKind::Shot,
    NodeKind::Line,
    NodeKind::Choice,
    NodeKind::Quest,
    NodeKind::Objective,
    NodeKind::Reward,
    NodeKind::Entry,
    NodeKind::Occasion,
    NodeKind::Relation,
    NodeKind::State,
    NodeKind::Def,
    NodeKind::Component,
    NodeKind::Expanded,
    NodeKind::Fact,
    NodeKind::Clock,
    NodeKind::Engine,
];

/// Whether a patch may name a node of this kind as a target (it still needs
/// an editable source anchor). Exhaustive: a new kind must be classified.
pub fn patch_targetable(kind: NodeKind) -> bool {
    match kind {
        NodeKind::Project
        | NodeKind::Fact
        | NodeKind::State
        | NodeKind::Relation
        | NodeKind::Engine
        | NodeKind::Occasion
        | NodeKind::Expanded => false,
        NodeKind::Document
        | NodeKind::Scene
        | NodeKind::Beat
        | NodeKind::Shot
        | NodeKind::Line
        | NodeKind::Choice
        | NodeKind::Quest
        | NodeKind::Objective
        | NodeKind::Reward
        | NodeKind::Entry
        | NodeKind::Def
        | NodeKind::Component
        | NodeKind::Clock => true,
    }
}

pub(crate) fn edit_op(op: &str) -> Option<&'static EditOp> {
    EDIT_OPS.iter().find(|spec| spec.op == op)
}

pub(crate) fn preserve_key(key: &str) -> Option<&'static PreserveKey> {
    PRESERVE_KEYS.iter().find(|spec| spec.key == key)
}

fn fields_json(fields: &[RequestField]) -> Vec<Value> {
    fields
        .iter()
        .map(|f| json!({ "name": f.name, "required": f.required, "type": f.ty }))
        .collect()
}

/// The request description `lute schema patch` reports under `inputs`:
/// `fields` (with `base`'s own `fields`), `ops`, `preserve`, `nodeKinds`.
pub fn patch_request_description() -> Value {
    let fields: Vec<Value> = REQUEST_FIELDS
        .iter()
        .map(|f| {
            let mut value = json!({ "name": f.name, "required": f.required, "type": f.ty });
            if f.name == "base" {
                value["fields"] = Value::Array(fields_json(BASE_FIELDS));
            }
            value
        })
        .collect();
    json!({
        "fields": fields,
        "ops": EDIT_OPS
            .iter()
            .map(|spec| json!({ "op": spec.op, "fields": fields_json(spec.fields) }))
            .collect::<Vec<_>>(),
        "preserve": PRESERVE_KEYS
            .iter()
            .map(|spec| json!({ "key": spec.key, "type": spec.ty }))
            .collect::<Vec<_>>(),
        "nodeKinds": NODE_KINDS
            .iter()
            .map(|kind| json!({ "kind": kind.as_str(), "targetable": patch_targetable(*kind) }))
            .collect::<Vec<_>>(),
    })
}

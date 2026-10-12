//! 0.41.0 §4.4 (`json-schema` feature): the index types' fields that embed a
//! shape the hand-written part of `schemas/lute-ir-*.schema.json` already
//! defines are `$ref`s to that definition (`schemars(schema_with = …)`), so
//! the generated index `$defs` never define a shape twice.

use schemars::{json_schema, Schema, SchemaGenerator};

fn hand(name: &str) -> Schema {
    json_schema!({ "$ref": format!("#/$defs/{name}") })
}

fn rows(name: &str) -> Schema {
    json_schema!({ "type": "array", "items": hand(name) })
}

pub(crate) fn cel_slot(_: &mut SchemaGenerator) -> Schema {
    hand("celSlot")
}

pub(crate) fn expr_node(_: &mut SchemaGenerator) -> Schema {
    hand("exprNode")
}

pub(crate) fn target_kind(_: &mut SchemaGenerator) -> Schema {
    hand("targetKind")
}

pub(crate) fn for_kind(_: &mut SchemaGenerator) -> Schema {
    hand("forKind")
}

pub(crate) fn clock_decl(_: &mut SchemaGenerator) -> Schema {
    hand("clockDecl")
}

pub(crate) fn entity_kinds(_: &mut SchemaGenerator) -> Schema {
    rows("entityKindEntry")
}

pub(crate) fn enums(_: &mut SchemaGenerator) -> Schema {
    rows("enumEntry")
}

pub(crate) fn relations(_: &mut SchemaGenerator) -> Schema {
    rows("relationEntry")
}

pub(crate) fn seed_facts(_: &mut SchemaGenerator) -> Schema {
    rows("seedFactEntry")
}

pub(crate) fn rules(_: &mut SchemaGenerator) -> Schema {
    rows("ruleEntry")
}

pub(crate) fn prereq_edges(_: &mut SchemaGenerator) -> Schema {
    rows("prereqEdgeEntry")
}

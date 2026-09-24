use std::collections::BTreeSet;

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::permissions::Permissions;
use lute_manifest::schema::StateShape;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::PathSegment;
use lute_syntax::ast::{Arm, Attr, AttrValue, ClipNode, Directive, Document, Node, Reward};

use crate::component_import::{ComponentDef, ComponentSet};
use crate::meta::{meta_key_span, TypedMeta};
use crate::CheckInput;

pub const E_PERMISSION_DIRECTIVE: &str = "E-PERMISSION-DIRECTIVE";
pub const E_PERMISSION_STATE: &str = "E-PERMISSION-STATE";
pub const E_PERMISSION_FACT: &str = "E-PERMISSION-FACT";
pub const E_PERMISSION_BRIDGE: &str = "E-PERMISSION-BRIDGE";
pub const E_PERMISSION_REWARD: &str = "E-PERMISSION-REWARD";
pub const E_PERMISSION_QUEST: &str = "E-PERMISSION-QUEST";

/// Re-check only the capability-permission contract for `input`.
///
/// This is the compiler's trust-boundary gate for a caller-supplied
/// [`crate::CheckResult`]. It deliberately does not re-run data-flow or other
/// project-reconciled checks. The unrestricted path returns before parsing.
pub fn check_permissions(input: &CheckInput) -> Vec<Diagnostic> {
    if input.snapshot.permissions.is_unrestricted() {
        return Vec::new();
    }

    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = crate::fold_env(&doc, input);
    let mut diagnostics = check_document_permissions(&doc, &folded.typed, input);
    diagnostics.sort_by(|a, b| {
        a.span
            .byte_start
            .cmp(&b.span.byte_start)
            .then_with(|| a.code.cmp(&b.code))
    });
    diagnostics
}

/// Shared implementation used by ordinary `check` after it has already parsed
/// and folded the document.
pub(crate) fn check_document_permissions(
    doc: &Document,
    typed: &TypedMeta,
    input: &CheckInput,
) -> Vec<Diagnostic> {
    let permissions = &input.snapshot.permissions;
    if permissions.is_unrestricted() {
        return Vec::new();
    }

    let mut checker = PermissionChecker {
        permissions,
        snapshot: &input.snapshot,
        components: &input.components,
        diagnostics: Vec::new(),
        using: Vec::new(),
    };

    checker.check_state_defaults(doc, typed, input);
    checker.check_seed_facts(doc, typed, input);

    for shot in &doc.shots {
        checker.walk_nodes(&shot.body);
    }
    for quest in &doc.quests {
        if !permissions.allows_quests() {
            checker.diagnostics.push(permission_diag(
                E_PERMISSION_QUEST,
                format!(
                    "quest declaration `{}` is forbidden by the effective `quests` permission ceiling",
                    quest.id
                ),
                quest.id_span,
            ));
        }
        checker.check_rewards(&quest.rewards);
        checker.walk_nodes(&quest.body);
    }
    // dsl 0.19.0 §4: entry bodies write state and facts (`::set`/`::assert`/
    // `::retract`) exactly as scene content does — the same write ceilings
    // apply. No declaration-level ceiling exists for entries.
    for entry in &doc.entries {
        checker.walk_nodes(&entry.body);
    }
    // dsl 0.23.0 §4: a lore `<beat>` bundle body is a scene shot body — the
    // same write ceilings apply.
    for beat in &doc.beats {
        checker.walk_nodes(&beat.body);
    }

    checker.diagnostics
}

struct PermissionChecker<'a> {
    permissions: &'a Permissions,
    snapshot: &'a CapabilitySnapshot,
    components: &'a ComponentSet,
    diagnostics: Vec<Diagnostic>,
    using: Vec<String>,
}

impl PermissionChecker<'_> {
    fn check_state_defaults(&mut self, doc: &Document, typed: &TypedMeta, input: &CheckInput) {
        // Imported defaults initialize the root document too. An inline override
        // replaces only an `extends` winner; a `uses` winner remains effective.
        for (path, decl) in &input.imports.state.decls {
            if typed.state.decls.contains_key(path)
                && input.imports.state_overridable.contains(path)
            {
                continue;
            }
            if decl.default.is_some() {
                self.check_state_write(
                    path,
                    doc.meta.span,
                    "imported state default initialization",
                );
            }
        }
        for (path, decl) in &typed.state.decls {
            if input.imports.state.decls.contains_key(path)
                && !input.imports.state_overridable.contains(path)
            {
                continue;
            }
            if decl.default.is_some() {
                self.check_state_write(
                    path,
                    meta_key_span(&doc.meta, path),
                    "state default initialization",
                );
            }
        }
    }

    fn check_seed_facts(&mut self, doc: &Document, typed: &TypedMeta, input: &CheckInput) {
        for fact in &input.imports.rel.facts {
            self.check_fact_write(&fact.fact.relation, doc.meta.span, "imported seed fact");
        }
        for fact in &typed.rel_facts {
            self.check_fact_write(&fact.fact.relation, fact.span, "seed fact");
        }
    }

    fn walk_nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(_) => {}
                Node::Directive(d) => {
                    self.check_directive(d);
                    if d.tag == "use" {
                        self.walk_component_use(d);
                    }
                }
                Node::Set(set) => {
                    self.check_directive_name("set", set.span);
                    self.check_state_write(&set.path, set.path_span, "`::set` write");
                }
                Node::Assert(assert) => {
                    self.check_directive_name("assert", assert.span);
                    self.check_fact_write(
                        &assert.pattern.relation,
                        assert.span,
                        "`::assert` write",
                    );
                }
                Node::Retract(retract) => {
                    self.check_directive_name("retract", retract.span);
                    self.check_fact_write(
                        &retract.pattern.relation,
                        retract.span,
                        "`::retract` write",
                    );
                }
                Node::Branch(branch) => {
                    self.check_state_write(
                        &format!("scene.choices.{}", branch.id),
                        branch.span,
                        "implicit branch selection write",
                    );
                    for choice in &branch.choices {
                        self.check_choice_into(choice);
                        self.walk_nodes(&choice.body);
                    }
                }
                Node::Hub(hub) => {
                    let id = string_attr(&hub.attrs, "id").unwrap_or_default();
                    self.check_state_write(
                        &format!("scene.choices.{id}"),
                        hub.span,
                        "implicit hub selection write",
                    );
                    for choice in &hub.choices {
                        self.check_state_write(
                            &format!("scene.visited.{id}.{}", choice.id),
                            choice.span,
                            "implicit hub visit write",
                        );
                        self.check_choice_into(choice);
                        self.walk_nodes(&choice.body);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.walk_nodes(body)
                            }
                        }
                    }
                }
                Node::Timeline(timeline) => {
                    for track in &timeline.tracks {
                        for clip in &track.clips {
                            match &clip.node {
                                ClipNode::Directive(d) => {
                                    self.check_directive(d);
                                    if d.tag == "use" {
                                        self.walk_component_use(d);
                                    }
                                }
                                ClipNode::Set(set) => {
                                    self.check_directive_name("set", set.span);
                                    self.check_state_write(
                                        &set.path,
                                        set.path_span,
                                        "timeline `::set` write",
                                    );
                                }
                            }
                        }
                    }
                }
                Node::Objective(objective) => {
                    self.check_rewards(&objective.rewards);
                    self.walk_nodes(&objective.body);
                }
                Node::On(on) => self.walk_nodes(&on.body),
            }
        }
    }

    fn check_directive(&mut self, directive: &Directive) {
        self.check_directive_name(&directive.tag, directive.span);

        let Some(decl) = self.snapshot.directive(&directive.tag).cloned() else {
            return;
        };
        if let Some(bridge) = &decl.bridge {
            if !self
                .permissions
                .allows_bridge(&bridge.service, &bridge.operation)
            {
                self.diagnostics.push(permission_diag(
                    E_PERMISSION_BRIDGE,
                    format!(
                        "bridge `{}/{}` used by `::{}` is forbidden by the effective `bridges` permission ceiling",
                        bridge.service, bridge.operation, directive.tag
                    ),
                    directive.span,
                ));
            }
        }
        if let Some(effects) = &decl.effects {
            for write in &effects.writes {
                match resolve_path(&write.scope, &write.path, &directive.attrs) {
                    Some(path) => self.check_state_write(
                        &path,
                        directive.span,
                        &format!("plugin `::{}` effect", directive.tag),
                    ),
                    None if state_writes_restricted(self.permissions) => {
                        self.diagnostics.push(permission_diag(
                            E_PERMISSION_STATE,
                            format!(
                                "plugin `::{}` has a state effect whose path cannot be resolved under the effective `stateWrites` permission ceiling",
                                directive.tag
                            ),
                            directive.span,
                        ));
                    }
                    None => {}
                }
            }
        }
        if let Some(state) = &decl.state {
            for slot in &state.declares {
                match resolve_path(&slot.scope, &slot.path, &directive.attrs) {
                    Some(base) => {
                        if let Some(shape) = self.snapshot.state_shapes.get(&slot.shape).cloned() {
                            self.check_shape_defaults(
                                &base,
                                &shape,
                                directive.span,
                                &mut BTreeSet::new(),
                            );
                        }
                    }
                    None if state_writes_restricted(self.permissions) => {
                        // A state shape may contain defaults. If the invocation
                        // cannot resolve its base, authorization must fail closed.
                        if self
                            .snapshot
                            .state_shapes
                            .get(&slot.shape)
                            .is_some_and(|shape| {
                                shape_has_defaults(shape, self.snapshot, &mut BTreeSet::new())
                            })
                        {
                            self.diagnostics.push(permission_diag(
                                E_PERMISSION_STATE,
                                format!(
                                    "plugin `::{}` initializes state through a path that cannot be resolved under the effective `stateWrites` permission ceiling",
                                    directive.tag
                                ),
                                directive.span,
                            ));
                        }
                    }
                    None => {}
                }
            }
        }
    }

    fn check_shape_defaults(
        &mut self,
        base: &str,
        shape: &StateShape,
        span: Span,
        visiting: &mut BTreeSet<String>,
    ) {
        if !visiting.insert(shape.name.clone()) {
            return;
        }
        for field in &shape.fields {
            let path = format!("{base}.{}", field.name);
            if field.default.is_some() {
                self.check_state_write(&path, span, "plugin state default initialization");
            }
            let nested = field
                .shape
                .as_ref()
                .and_then(|name| self.snapshot.state_shapes.get(name))
                .cloned();
            if let Some(nested) = nested {
                self.check_shape_defaults(&path, &nested, span, visiting);
            }
        }
        visiting.remove(&shape.name);
    }

    fn check_choice_into(&mut self, choice: &lute_syntax::ast::Choice) {
        let Some(attr) = choice.attrs.iter().find(|attr| attr.key == "into") else {
            return;
        };
        match &attr.value {
            AttrValue::Str(path) => {
                self.check_state_write(path, attr.value_span, "choice `into` write")
            }
            _ if state_writes_restricted(self.permissions) => self.diagnostics.push(
                permission_diag(
                    E_PERMISSION_STATE,
                    "choice `into` write target cannot be resolved under the effective `stateWrites` permission ceiling".to_string(),
                    attr.value_span,
                ),
            ),
            _ => {}
        }
    }

    fn check_rewards(&mut self, rewards: &[Reward]) {
        if self.permissions.allows_rewards() {
            return;
        }
        for reward in rewards {
            self.diagnostics.push(permission_diag(
                E_PERMISSION_REWARD,
                format!(
                    "reward declaration `{}` is forbidden by the effective `rewards` permission ceiling",
                    reward.kind
                ),
                reward.span,
            ));
        }
    }

    fn check_directive_name(&mut self, name: &str, span: Span) {
        if !self.permissions.allows_directive(name) {
            self.diagnostics.push(permission_diag(
                E_PERMISSION_DIRECTIVE,
                format!(
                    "directive `::{name}` is forbidden by the effective `directives` permission ceiling"
                ),
                span,
            ));
        }
    }

    fn check_state_write(&mut self, path: &str, span: Span, source: &str) {
        if !self.permissions.allows_state_write(path) {
            self.diagnostics.push(permission_diag(
                E_PERMISSION_STATE,
                format!(
                    "{source} to `{path}` is forbidden by the effective `stateWrites` permission ceiling"
                ),
                span,
            ));
        }
    }

    fn check_fact_write(&mut self, relation: &str, span: Span, source: &str) {
        if relation.is_empty() {
            return;
        }
        if !self.permissions.allows_fact_write(relation) {
            self.diagnostics.push(permission_diag(
                E_PERMISSION_FACT,
                format!(
                    "{source} to relation `{relation}` is forbidden by the effective `factWrites` permission ceiling"
                ),
                span,
            ));
        }
    }

    fn walk_component_use(&mut self, directive: &Directive) {
        let Some(name) = string_attr(&directive.attrs, "component") else {
            return;
        };
        let Some(def) = self.components.table.get(name).cloned() else {
            return;
        };
        if self.using.iter().any(|active| active == name) {
            return;
        }

        self.using.push(name.to_string());
        let start = self.diagnostics.len();
        for shot in &def.body.shots {
            self.walk_nodes(&shot.body);
        }
        let nested = self.diagnostics.split_off(start);
        self.using.pop();

        for diagnostic in nested {
            self.diagnostics.push(component_permission_diag(
                diagnostic,
                name,
                &def,
                directive.span,
            ));
        }
    }
}

/// Resolve a manifest-authored path template against one directive invocation.
/// Slot declarations, effect writes and permission enforcement all use this
/// exact convention; an absent or non-string `fromAttr` fails the whole path.
pub(crate) fn resolve_path(
    scope: &str,
    segments: &[PathSegment],
    attrs: &[Attr],
) -> Option<String> {
    let mut parts = Vec::with_capacity(segments.len() + 1);
    parts.push(scope.to_string());
    for segment in segments {
        match segment {
            PathSegment::Literal(value) => parts.push(value.clone()),
            PathSegment::FromAttr { from_attr } => {
                parts.push(string_attr(attrs, &from_attr.name)?.to_string())
            }
        }
    }
    Some(parts.join("."))
}

fn string_attr<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs.iter().find_map(|attr| {
        if attr.key != key {
            return None;
        }
        match &attr.value {
            AttrValue::Str(value) => Some(value.as_str()),
            _ => None,
        }
    })
}

fn state_writes_restricted(permissions: &Permissions) -> bool {
    permissions.layers.iter().any(|layer| {
        layer
            .state_writes
            .as_ref()
            .is_some_and(|allowed| !allowed.contains("*"))
    })
}

fn shape_has_defaults(
    shape: &StateShape,
    snapshot: &CapabilitySnapshot,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(shape.name.clone()) {
        return false;
    }
    let has_default = shape.fields.iter().any(|field| {
        field.default.is_some()
            || field
                .shape
                .as_ref()
                .and_then(|name| snapshot.state_shapes.get(name))
                .is_some_and(|nested| shape_has_defaults(nested, snapshot, visiting))
    });
    visiting.remove(&shape.name);
    has_default
}

fn permission_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn component_permission_diag(
    mut diagnostic: Diagnostic,
    name: &str,
    def: &ComponentDef,
    invocation_span: Span,
) -> Diagnostic {
    let inner = Diagnostic {
        code: diagnostic.code.clone(),
        severity: diagnostic.severity,
        message: diagnostic.message.clone(),
        span: diagnostic.span,
        layer: diagnostic.layer,
        fixits: Vec::new(),
        provenance: None,
        covered: diagnostic.covered.clone(),
        related: diagnostic.related.clone(),
    };
    diagnostic.message = format!(
        "component `{name}` ({}): {}",
        def.src.display(),
        diagnostic.message
    );
    diagnostic.span = invocation_span;
    diagnostic.fixits.clear();
    diagnostic.covered.clear();
    diagnostic.related = vec![RelatedDiagnostic {
        file: def.src.display().to_string(),
        diagnostic: inner,
    }];
    diagnostic
}

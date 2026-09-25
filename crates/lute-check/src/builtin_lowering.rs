//! A plugin directive declared `lower: { kind: builtin, name: <hook> }`
//! (plugin §9) runs the core builtin that hook names — `::wipe` with
//! `lower: { kind: builtin, name: clearStage }` IS a `::clear`. The checker
//! validates it under its own tag and declaration; the compiler and trace
//! then see it under the core tag, so every stage, lowering and walk rule
//! keyed on the core tag applies to it unchanged.

use lute_manifest::schema::Lowering;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, ClipNode, Directive, Document, Node};

/// The core directive a builtin lowering hook belongs to.
fn core_tag(hook: &str) -> Option<&'static str> {
    Some(match hook {
        "autoStage" => "auto",
        "cameraTransform" => "camera",
        "clearStage" => lute_manifest::core::CLEAR_DIRECTIVE,
        "end" => lute_manifest::core::END_DIRECTIVE,
        "mark" => lute_manifest::core::MARK_DIRECTIVE,
        "next" => lute_manifest::core::NEXT_DIRECTIVE,
        _ => return None,
    })
}

/// Retag every directive in `doc` whose declaration lowers through a core
/// builtin hook to that hook's core tag. Run after checking, before
/// normalization (compile, trace).
pub fn canonicalize_builtin_directives(doc: &mut Document, snapshot: &CapabilitySnapshot) {
    let retag = |d: &mut Directive| {
        let hook = match snapshot.directive(&d.tag).map(|decl| &decl.lower) {
            Some(Lowering::Builtin { name, .. }) => core_tag(name),
            _ => None,
        };
        if let Some(tag) = hook.filter(|t| *t != d.tag) {
            d.tag = tag.to_string();
        }
    };
    fn walk(nodes: &mut [Node], f: &impl Fn(&mut Directive)) {
        for node in nodes {
            match node {
                Node::Directive(d) => f(d),
                Node::Branch(b) => b.choices.iter_mut().for_each(|c| walk(&mut c.body, f)),
                Node::Hub(h) => h.choices.iter_mut().for_each(|c| walk(&mut c.body, f)),
                Node::Match(m) => {
                    for arm in &mut m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        walk(body, f);
                    }
                }
                Node::On(o) => walk(&mut o.body, f),
                Node::Objective(o) => walk(&mut o.body, f),
                Node::Timeline(tl) => {
                    for clip in tl.tracks.iter_mut().flat_map(|t| &mut t.clips) {
                        if let ClipNode::Directive(d) = &mut clip.node {
                            f(d);
                        }
                    }
                }
                Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }
    for shot in &mut doc.shots {
        walk(&mut shot.body, &retag);
    }
    for q in &mut doc.quests {
        walk(&mut q.body, &retag);
    }
    for e in &mut doc.entries {
        walk(&mut e.body, &retag);
    }
    for b in &mut doc.beats {
        walk(&mut b.body, &retag);
    }
}

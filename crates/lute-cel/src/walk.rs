//! Exhaustive traversal of the CEL parser's expression tree.
//!
//! [`walk`] is deliberately a small structural primitive.  Callers own the
//! semantic policy (for example, whether a recognised host call is a leaf),
//! and return [`Flow::Skip`] for that policy.  The implementation is the one
//! place that enumerates the parser's expression and entry variants, so a new
//! CEL AST node cannot silently be omitted from checker passes.

use cel_parser::ast::{EntryExpr, Expr};

/// A node visited by [`walk`].
#[derive(Clone, Copy, Debug)]
pub enum Node<'a> {
    /// An expression node, including the root and every expression child.
    Expr(&'a Expr),
    /// A map or struct entry whose expression children follow it.
    Entry(&'a EntryExpr),
}

/// Controls traversal after a node callback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flow {
    /// Visit this node's children in source order.
    #[default]
    Continue,
    /// Do not visit this node's children, but continue with the caller's next
    /// sibling.
    Skip,
    /// End the whole traversal immediately.
    Stop,
}

/// Visit every expression and map/struct entry below `expr` in pre-order.
///
/// The match is intentionally exhaustive over `cel-parser`'s AST.  A callback
/// can implement pass-specific stop rules without copying this structural
/// recursion: [`Flow::Skip`] makes a node a semantic leaf and [`Flow::Stop`]
/// aborts the complete walk.
pub fn walk(expr: &Expr, visit: &mut impl FnMut(Node<'_>) -> Flow) -> Flow {
    let flow = visit(Node::Expr(expr));
    if flow != Flow::Continue {
        return match flow {
            Flow::Stop => Flow::Stop,
            Flow::Skip => Flow::Continue,
            Flow::Continue => unreachable!("flow was checked above"),
        };
    }

    match expr {
        Expr::Unspecified | Expr::Ident(_) | Expr::Literal(_) => Flow::Continue,
        Expr::Call(call) => {
            if let Some(target) = &call.target {
                if walk(&target.expr, visit) == Flow::Stop {
                    return Flow::Stop;
                }
            }
            for arg in &call.args {
                if walk(&arg.expr, visit) == Flow::Stop {
                    return Flow::Stop;
                }
            }
            Flow::Continue
        }
        Expr::Comprehension(comprehension) => {
            for child in [
                &comprehension.iter_range,
                &comprehension.accu_init,
                &comprehension.loop_cond,
                &comprehension.loop_step,
                &comprehension.result,
            ] {
                if walk(&child.expr, visit) == Flow::Stop {
                    return Flow::Stop;
                }
            }
            Flow::Continue
        }
        Expr::List(list) => {
            for element in &list.elements {
                if walk(&element.expr, visit) == Flow::Stop {
                    return Flow::Stop;
                }
            }
            Flow::Continue
        }
        Expr::Map(map) => walk_entries(&map.entries, visit),
        Expr::Select(select) => walk(&select.operand.expr, visit),
        Expr::Struct(struct_expr) => walk_entries(&struct_expr.entries, visit),
    }
}

fn walk_entries(entries: &[cel_parser::ast::IdedEntryExpr], visit: &mut impl FnMut(Node<'_>) -> Flow) -> Flow {
    for ided in entries {
        let flow = visit(Node::Entry(&ided.expr));
        if flow == Flow::Stop {
            return Flow::Stop;
        }
        // Entry::Skip is a semantic policy for the entry, so it intentionally
        // suppresses both its key and value (or its struct field value).
        if flow == Flow::Skip {
            continue;
        }
        let result = match &ided.expr {
            EntryExpr::MapEntry(entry) => {
                if walk(&entry.key.expr, visit) == Flow::Stop {
                    Flow::Stop
                } else {
                    walk(&entry.value.expr, visit)
                }
            }
            EntryExpr::StructField(field) => walk(&field.value.expr, visit),
        };
        if result == Flow::Stop {
            return Flow::Stop;
        }
    }
    Flow::Continue
}


#[cfg(test)]
mod tests {
    use super::*;
    use cel_parser::ast::{CallExpr, ComprehensionExpr, IdedEntryExpr, IdedExpr, MapEntryExpr, MapExpr, SelectExpr, StructExpr, StructFieldExpr};
    use cel_parser::reference::Val;

    fn id(expr: Expr) -> IdedExpr {
        IdedExpr { id: 0, expr }
    }

    #[test]
    fn visits_every_expression_and_entry_kind() {
        let expr = Expr::Call(CallExpr {
            func_name: "root".to_string(),
            target: Some(Box::new(id(Expr::Select(SelectExpr {
                operand: Box::new(id(Expr::Ident("target".to_string()))),
                field: "field".to_string(),
                test: false,
            })))),
            args: vec![
                id(Expr::List(cel_parser::ast::ListExpr {
                    elements: vec![id(Expr::Literal(Val::Int(1))), id(Expr::Unspecified)],
                })),
                id(Expr::Map(MapExpr {
                    entries: vec![IdedEntryExpr {
                        id: 0,
                        expr: EntryExpr::MapEntry(MapEntryExpr {
                            key: id(Expr::Literal(Val::String("key".to_string()))),
                            value: id(Expr::Struct(StructExpr {
                                type_name: "Thing".to_string(),
                                entries: vec![IdedEntryExpr {
                                    id: 0,
                                    expr: EntryExpr::StructField(StructFieldExpr {
                                        field: "value".to_string(),
                                        value: id(Expr::Ident("value".to_string())),
                                        optional: false,
                                    }),
                                }],
                            })),
                            optional: false,
                        }),
                    }],
                })),
                id(Expr::Comprehension(ComprehensionExpr {
                    iter_range: Box::new(id(Expr::Ident("items".to_string()))),
                    iter_var: "item".to_string(),
                    iter_var2: None,
                    accu_var: "acc".to_string(),
                    accu_init: Box::new(id(Expr::Literal(Val::Int(0)))),
                    loop_cond: Box::new(id(Expr::Literal(Val::Boolean(true)))),
                    loop_step: Box::new(id(Expr::Ident("step".to_string()))),
                    result: Box::new(id(Expr::Ident("result".to_string()))),
                })),
            ],
        });
        let mut exprs = 0;
        let mut entries = 0;
        walk(&expr, &mut |node| {
            match node {
                Node::Expr(_) => exprs += 1,
                Node::Entry(_) => entries += 1,
            }
            Flow::Continue
        });
        assert_eq!(exprs, 16);
        assert_eq!(entries, 2);
    }

    #[test]
    fn skip_and_stop_are_explicit() {
        let expr = Expr::List(cel_parser::ast::ListExpr {
            elements: vec![id(Expr::Ident("a".to_string())), id(Expr::Ident("b".to_string()))],
        });
        let mut seen = Vec::new();
        walk(&expr, &mut |node| {
            let Node::Expr(expr) = node else { return Flow::Continue };
            if let Expr::Ident(name) = expr {
                seen.push(name.clone());
            }
            if matches!(expr, Expr::List(_)) { Flow::Skip } else { Flow::Continue }
        });
        assert!(seen.is_empty());

        let mut seen = 0;
        assert_eq!(walk(&expr, &mut |_| { seen += 1; Flow::Stop }), Flow::Stop);
        assert_eq!(seen, 1);
    }
}

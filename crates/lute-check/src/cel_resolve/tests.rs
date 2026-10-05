use super::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use lute_syntax::ast::CelKind;
    use std::collections::BTreeSet;

    fn test_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn env_with_defs(names: &[&str]) -> Env {
        Env {
            defs: names.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
            ..Env::default()
        }
    }

    fn env_with_def(name: &str, ty: Type) -> Env {
        Env {
            defs: std::iter::once(name.to_string()).collect(),
            def_types: std::iter::once((name.to_string(), ty)).collect(),
            ..Env::default()
        }
    }

    fn env_with_state(path: &str, ty: Type) -> Env {
        let mut schema = crate::meta::StateSchema::default();
        schema.decls.insert(
            path.to_string(),
            crate::meta::StateDecl {
                ty,
                default: None,
                namespace: crate::meta::Namespace::Scene,
                owner: None,
            },
        );
        Env {
            state: schema,
            ..Env::default()
        }
    }

    fn mk_ctx(env: &Env) -> Ctx<'_> {
        Ctx {
            env,
            in_match: false,
            match_subject: None,
        }
    }

    fn mk_ctx_in_match(env: &Env) -> Ctx<'_> {
        Ctx {
            env,
            in_match: true,
            match_subject: None,
        }
    }

    /// Build a `Condition` slot and parse it into a fresh arena so `ast` is `Some`.
    fn cel_slot_condition(raw: &str) -> CelSlot {
        let mut slot = CelSlot::raw(CelKind::Condition, raw.to_string(), test_span());
        let mut arena = CelArena::default();
        if let Ok(h) = lute_cel::parse_slot(&mut arena, &slot.raw, slot.span.byte_start) {
            slot.ast = Some(h);
        }
        slot
    }

    /// Re-parse the slot's raw into a fresh arena, reproducing the same handle
    /// index the slot recorded (each parse into an empty arena yields handle 0).
    fn arena_for(slot: &CelSlot) -> CelArena {
        let mut arena = CelArena::default();
        let _ = lute_cel::parse_slot(&mut arena, &slot.raw, slot.span.byte_start);
        arena
    }

    #[test]
    fn dollar_outside_match_errors() {
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("$ == 'x'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-DOLLAR-OUTSIDE-MATCH"));
    }

    #[test]
    fn undeclared_ref_errors() {
        let env = env_with_defs(&["fond"]);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@warm");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-UNDECLARED-REF"));
    }

    #[test]
    fn choicelog_read_in_guard_errors() {
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        let slot = cel_slot_condition("run.choiceLog.ep02.couch == 'help'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-CHOICELOG-READ"));
    }

    #[test]
    fn ref_type_mismatch_flags() {
        let env = env_with_def("num", Type::Int);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num"); // referenced in a bool position
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_compatible_is_clean() {
        let env = env_with_def("flag", Type::Bool);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@flag");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_unknown_expected_no_false_positive() {
        let env = env_with_def("num", Type::Int);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None); // expected unknown
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_string_family_clean() {
        // def produces Str used where an Enum is expected -> string family -> no flag
        let env = env_with_def("s", Type::Str);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@s");
        let d = check_cel_slot(
            &slot,
            &arena_for(&slot),
            &ctx,
            Some(&ExpectedType::Ty(Type::Enum(vec!["a".into(), "b".into()]))),
        );
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_id_type_clean() {
        // expected an id type -> always compatible
        let env = env_with_def("n", Type::Int);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@n");
        let d = check_cel_slot(
            &slot,
            &arena_for(&slot),
            &ctx,
            Some(&ExpectedType::Ty(Type::ProviderRef("prov".into()))),
        );
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_undeclared_ref_no_reftype() {
        // name not in ctx.defs -> E-UNDECLARED-REF, NOT E-REF-TYPE (no double report)
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@ghost");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(d.iter().any(|x| x.code == "E-UNDECLARED-REF"));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_compound_expr_no_false_positive() {
        // `@num > 0` in a bool slot: @num (Number) types a numeric subexpression;
        // the whole expression is boolean -> must NOT flag E-REF-TYPE.
        let env = env_with_def("num", Type::Int);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num > 0");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(
            !d.iter().any(|x| x.code == "E-REF-TYPE"),
            "compound expression must not flag E-REF-TYPE; got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    /// G-5: a bool slot holding one bare state path of another type is
    /// never true — `E-REF-TYPE` with a compare example for its type, the
    /// same verdict a whole-slot `@def` of that type gets. A bool path, a
    /// comparison and an unknown expected type stay clean.
    #[test]
    fn bare_non_bool_path_in_a_bool_slot_is_ref_type() {
        let cases = [
            (Type::Int, "`user.day > 0`"),
            (Type::Str, "`user.day != ''`"),
            (
                Type::Enum(vec!["dawn".into(), "dusk".into()]),
                "`user.day == 'dawn'`",
            ),
        ];
        for (ty, example) in cases {
            let env = env_with_state("user.day", ty);
            let ctx = mk_ctx(&env);
            let slot = cel_slot_condition("user.day");
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
            let hit = d
                .iter()
                .find(|x| x.code == "E-REF-TYPE")
                .expect("E-REF-TYPE");
            assert!(hit.message.contains(example), "{}", hit.message);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"), "{d:?}");
        }
        let env = env_with_state("user.open", Type::Bool);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("user.open");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"), "{d:?}");
        let env = env_with_state("user.day", Type::Int);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("user.day > 2");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"), "{d:?}");
    }

    #[test]
    fn out_of_profile_call_rejected() {
        // dsl §8.4: the Lute-CEL environment is CLOSED — only operators/literals/
        // lists/`?:`/`in`/`has()` and the declared list-form host calls are allowed.
        // Any other function call or comprehension macro is `E-CEL-PROFILE`.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for raw in [
            "size(scene.x) > 0",
            "[1, 2].exists(x, x > 0)",
            "matches(a, b)",
        ] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "expected E-CEL-PROFILE for `{raw}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn in_profile_exprs_pass() {
        // The closed set never trips the gate: `has`, `in`, arithmetic +
        // comparison operators, and the ternary conditional.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for ok in [
            "has(scene.x)",
            "$ in ['a', 'b']",
            "scene.n + 1 > 2",
            // the ternary conditional operator itself is in profile; operands are
            // state paths / literals (bare idents are NOT in profile — see
            // `bare_ident_rejected`).
            "scene.n > 0 ? run.a : run.b",
        ] {
            let slot = cel_slot_condition(ok);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "unexpected E-CEL-PROFILE for `{ok}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bare_ident_rejected() {
        // dsl §8.4/§9.1: a bare identifier that is not a state-tier root, the `$`
        // subject, or a `@ref` is a free variable reference — out of profile.
        // `when="typo"` and a non-state-rooted `isSet(foo.bar)` both flag.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for raw in ["typo", "isSet(foo.bar)", "foo.bar", "a && b"] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "bare identifier `{raw}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn legal_ident_roots_pass() {
        // The legal roots never trip the gate: state paths (any tier), the `$`
        // subject, a `has()` guard, and CEL keyword literals.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for ok in [
            "scene.x == 1",
            "run.y",
            "user.z || app.w",
            "$ == 'gold'",
            "has(scene.x)",
            "true",
            "false ? 1 : 2",
        ] {
            let slot = cel_slot_condition(ok);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "legal root `{ok}` must not trip E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
        let slot = cel_slot_condition("false ? 1 : null");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().any(|e| e.code == E_CEL_PROFILE),
            "`null` literal must trip E-CEL-PROFILE under 0.32, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn def_ref_call_form_not_flagged() {
        // A parameterized def reference `@name(args)` is a COMPILE-TIME macro
        // invocation (dsl §8.1), not a runtime CEL function call. The marker
        // re-parse gives it a `REF_MARKER`-prefixed name so it is exempt — while a
        // same-named runtime call is not (see `ref_call_not_shadowed_by_at_ref`).
        for (env, raw) in [
            (env_with_defs(&["atLeast"]), "@atLeast(2)"),
            (Env::default(), "@ghost(1)"), // undeclared ref -> E-UNDECLARED-REF only
            (env_with_defs(&["pick"]), "@pick(scene.n, 3) > 0"),
        ] {
            let ctx = mk_ctx(&env);
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "def-ref call `{raw}` must not trip E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn ref_call_not_shadowed_by_at_ref() {
        // Bypass (1): a real runtime call must NOT be exempted just because a
        // same-named `@ref` appears in the slot. `@gate` is a bare ref (Ident);
        // `gate(scene.x)` is a genuine out-of-profile call -> E-CEL-PROFILE.
        let env = env_with_defs(&["gate"]);
        let ctx = mk_ctx(&env);
        for raw in ["@gate && gate(scene.x)", "@gate(1) && gate(2)"] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "runtime `gate(...)` must flag E-CEL-PROFILE despite `@gate` in `{raw}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn has_call_form_rejected_but_macro_ok() {
        // Bypass (2): the valid `has(path)` macro parses as an `Expr::Select`, so
        // any residual `Call` named `has` (wrong arity / receiver form) is NOT the
        // macro and must flag; the real macro must stay clean.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        for bad in ["has(a, b)", "scene.x.has()"] {
            let slot = cel_slot_condition(bad);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "non-macro `has` call `{bad}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
        let slot = cel_slot_condition("has(scene.x)");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().all(|e| e.code != E_CEL_PROFILE),
            "valid has() macro must stay clean, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    /// dsl 0.24.0 §1 (T2-1): integer `%` is in the profile. Before 0.24 it was
    /// `E-CEL-PROFILE` (no integer domain); now a `number` operand is clean
    /// and only a non-integer operand — a non-`number` or a fractional
    /// literal — is `E-CEL-TYPE`.
    #[test]
    fn integer_modulo_in_profile_non_integer_is_cel_type() {
        let mut env = env_with_state("run.day", Type::Int);
        env.state.decls.insert(
            "run.flag".to_string(),
            crate::meta::StateDecl {
                ty: Type::Bool,
                default: None,
                namespace: crate::meta::Namespace::Run,
                owner: None,
            },
        );
        let ctx = mk_ctx(&env);
        let codes = |raw: &str| {
            let slot = cel_slot_condition(raw);
            check_cel_slot(&slot, &arena_for(&slot), &ctx, None)
                .into_iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        };
        for ok in [
            "run.day % 7 == 0",
            "(run.day - 1) % 7 == 3",
            "run.day % 7.0 == 0",
            // dsl 0.28.0 §1: a whole condition must be a bool, so the
            // arithmetic is compared rather than standing alone.
            "run.day + 1 > 0",
            "run.day - 1 * 2 / 3 > 0",
        ] {
            let c = codes(ok);
            assert!(
                !c.iter().any(|x| x == E_CEL_PROFILE),
                "`{ok}` must be clean, got {c:?}"
            );
        }
        for bad in [
            "run.day % 2.5 == 0",
            "run.day % -0.5 == 0",
            "'a' % 2 == 0",
            "run.flag % 2 == 0",
            "(run.day % 2.5) % 2 > 0",
        ] {
            let c = codes(bad);
            assert_eq!(
                c.iter().filter(|x| *x == E_CEL_TYPE).count(),
                1,
                "`{bad}` must flag E-CEL-TYPE once, got {c:?}"
            );
            assert!(!c.iter().any(|x| x == E_CEL_PROFILE), "`{bad}`: {c:?}");
        }
        let slot = cel_slot_condition("run.day % 2.5 == 0");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter()
                .any(|x| x.message.contains("`2.5` is not an integer")),
            "{d:?}"
        );
    }

    #[test]
    fn hyphenated_path_is_one_path_ident_error() {
        let mut env = env_with_state("run.day", Type::Int);
        env.state.decls.insert(
            "run.other".to_string(),
            crate::meta::StateDecl {
                ty: Type::Int,
                default: None,
                namespace: crate::meta::Namespace::Scene,
                owner: None,
            },
        );
        let ctx = mk_ctx(&env);
        let check = |raw: &str| {
            let slot = cel_slot_condition(raw);
            check_cel_slot(&slot, &arena_for(&slot), &ctx, None)
        };
        let d = check("quest.lamp-duty.state == 'active'");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].code, crate::cel_paths::E_PATH_IDENT);
        assert!(
            d[0].message.contains("write `quest[\"lamp-duty\"].state`"),
            "{}",
            d[0].message
        );
        let d = check("run.day > 0 && run.lamp-duty-log.count > 1");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(
            d[0].message.contains("`run[\"lamp-duty-log\"].count`"),
            "{}",
            d[0].message
        );
        // A real subtraction stays one.
        for ok in [
            "run.day-1 > 0",
            "run.day-run.other > 0",
            "run.day - run.other > 0",
        ] {
            let d = check(ok);
            assert!(d.is_empty(), "`{ok}`: {d:?}");
        }
    }

    #[test]
    fn reserved_marker_in_raw_rejected() {
        // A hand-written identifier beginning with the reserved marker token must
        // not masquerade as an exempt `@ref` — it is itself out of profile.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let raw = format!("{}x + 1", lute_cel::REF_MARKER);
        let slot = cel_slot_condition(&raw);
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().any(|e| e.code == E_CEL_PROFILE),
            "reserved-marker identifier `{raw}` must flag E-CEL-PROFILE, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }



    #[test]
    fn isset_in_any_shape_is_out_of_profile() {
        // `isSet` is not part of the 0.32 CEL profile; presence is `has()`.
        // Receiver, arity, and argument shape must all remain rejected.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        for raw in [
            "scene.x.isSet()",
            "isSet(a, b)",
            "isSet(1 + 2)",
            "isSet(scene.x + 1)",
            "isSet(run.y)",
        ] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "`{raw}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn undeclared_read_near_a_declared_path_suggests_it() {
        // dsl 0.5.0 §2.2 "did you mean": `scene.trsut` is one transposition
        // away from the declared `scene.trust`.
        let env = env_with_state("scene.trust", Type::Bool);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("scene.trsut");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        let e = d
            .iter()
            .find(|e| e.code == "E-UNDECLARED")
            .unwrap_or_else(|| panic!("{d:?}"));
        assert!(
            e.message.contains("did you mean `scene.trust`"),
            "{}",
            e.message
        );
    }
}

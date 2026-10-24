//! spec 0.49.0 §9.3 — a `per:` family is stored once, not member by member.

const MEMBERS: usize = 100_000;

/// A schema document: `npc` lists the first half of `MEMBERS` ids, its
/// sub-kind `bonded` adds the second half. `user.seen` is a family over
/// `npc`, `user.bond` one over `bonded` with one member's own default.
fn schema() -> String {
    let ids = |range: std::ops::Range<usize>| -> String {
        range
            .map(|i| format!("m{i:06}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let half = MEMBERS / 2;
    format!(
        "---\nstate:\n  user.seen: {{ type: bool, default: false, per: npc }}\n  \
         user.bond: {{ type: int, default: {{ m099999: 5, _: 0 }}, per: bonded }}\n\
         entities:\n  npc: {{ members: [{}] }}\n  \
         bonded: {{ subsetOf: npc, members: [{}] }}\n---\n",
        ids(0..half),
        ids(half..MEMBERS),
    )
}

#[test]
fn a_family_over_a_100_000_member_kind_stores_no_member_decl() {
    let text = schema();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta, diags) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    // A frontmatter without `kind:` reads as a scene, which needs an `id:`.
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E-META-MISSING"], "{diags:?}");
    let decls = &meta.state.decls;

    assert!(
        decls.explicit().keys().all(|k| !k.starts_with("user.")),
        "a family member is stored as its own decl: {:?}",
        decls.explicit().keys().find(|k| k.starts_with("user."))
    );
    let seen = &decls.families()["user.seen"];
    assert_eq!(seen.kind(), "npc");
    assert_eq!(
        seen.members().len(),
        MEMBERS,
        "`npc` includes `bonded`'s members"
    );
    let bond = &decls.families()["user.bond"];
    assert_eq!(bond.members().len(), MEMBERS / 2);

    // `npc`'s own member and one its sub-kind adds.
    for path in ["user.seen.m000000", "user.seen.m099999"] {
        let d = decls
            .get(path)
            .unwrap_or_else(|| panic!("{path} undeclared"));
        assert_eq!(d.default, Some(lute_manifest::types::Literal::Bool(false)));
    }
    assert_eq!(
        decls
            .get("user.bond.m099999")
            .and_then(|d| d.default.clone()),
        Some(lute_manifest::types::Literal::Int(5)),
    );
    assert_eq!(
        decls
            .get("user.bond.m050000")
            .and_then(|d| d.default.clone()),
        Some(lute_manifest::types::Literal::Int(0)),
    );
    assert!(
        decls.get("user.bond.m000000").is_none(),
        "not a `bonded` member"
    );
    assert!(decls.get("user.seen.m100000").is_none());

    // Iteration still names every member path, in byte order.
    let keys: Vec<String> = decls
        .keys()
        .filter(|k| k.starts_with("user."))
        .map(|k| k.into_owned())
        .collect();
    assert_eq!(keys.len(), MEMBERS + MEMBERS / 2);
    assert!(keys.windows(2).all(|w| w[0] < w[1]));
}

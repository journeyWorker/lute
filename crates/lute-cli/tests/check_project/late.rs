//! Entry, fact-envelope, WIP, and cross-document diagnostic contracts.

use super::*;

/// Every `E-ENTRY-ID-DUP` across `v`'s per-file results AND its project list,
/// as `(path suffix, byte_start)` so a per-file twin of a project report is
/// visible as a repeated pair.
fn entry_dup_sites(v: &serde_json::Value) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for f in v["files"].as_array().unwrap() {
        let path = f["path"]
            .as_str()
            .or(f["file"].as_str())
            .unwrap_or_default();
        for d in f["diagnostics"].as_array().unwrap() {
            if d["code"] == "E-ENTRY-ID-DUP" {
                out.push((path.to_string(), d["span"]["byte_start"].as_u64().unwrap()));
            }
        }
    }
    for d in v["project_diagnostics"].as_array().unwrap() {
        if d["code"] == "E-ENTRY-ID-DUP" {
            out.push((
                d["path"].as_str().unwrap().to_string(),
                d["span"]["byte_start"].as_u64().unwrap(),
            ));
        }
    }
    out
}

/// `a.lute` declares `x` twice (per-file `check` already reports the second)
/// and `b.lute` declares it again. The project pass reports every occurrence
/// past the first exactly once — the per-file twin is suppressed.
#[test]
fn check_project_reports_each_duplicate_entry_id_exactly_once() {
    let dir = temp_dir("entry-id-dup");
    let a = "---\nkind: lore\n---\n<entry id=\"x\">\n@narrator: one\n</entry>\n\
             <entry id=\"x\">\n@narrator: two\n</entry>\n";
    let b = "---\nkind: lore\n---\n<entry id=\"x\">\n@narrator: three\n</entry>\n";
    write(&dir, "a.lute", a);
    write(&dir, "b.lute", b);

    // Red proof that a per-file twin exists to be suppressed.
    let single = run(&["check", dir.join("a.lute").to_str().unwrap(), "--json"]);
    let sv: serde_json::Value = serde_json::from_slice(&single.stdout).unwrap();
    assert_eq!(
        sv["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == "E-ENTRY-ID-DUP")
            .count(),
        1,
        "{sv}"
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let sites = entry_dup_sites(&v);
    let second_in_a = a.rfind("<entry").unwrap() as u64;
    let mut found: Vec<(bool, u64)> = sites
        .iter()
        .map(|(p, s)| (p.ends_with("a.lute"), *s))
        .collect();
    found.sort();
    assert_eq!(sites.len(), 2, "one report per non-first occurrence: {v}");
    assert!(
        found[1].0 && found[1].1 >= second_in_a,
        "a.lute's second `x`: {v}"
    );
    assert!(!found[0].0, "b.lute's `x`: {v}");
}

/// A scene reading `entry.nope.read` when no lore document declares `nope`
/// draws `W-ENTRY-REF-UNKNOWN` (a warning: exit stays 0); a read of a
/// declared entry does not.
#[test]
fn check_project_warns_on_an_unknown_entry_read() {
    let dir = temp_dir("entry-ref-unknown");
    write(
        &dir,
        "lore.lute",
        "---\nkind: lore\n---\n<entry id=\"note\">\n@narrator: hi\n</entry>\n",
    );
    write(
        &dir,
        "scene.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         @a{when=\"entry.note.read\"}: known.\n\
         @a{when=\"entry.nope.read\"}: unknown.\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let warns: Vec<&serde_json::Value> = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "W-ENTRY-REF-UNKNOWN")
        .collect();
    assert_eq!(warns.len(), 1, "{v}");
    assert_eq!(warns[0]["severity"], "warning");
    assert!(
        warns[0]["path"].as_str().unwrap().ends_with("scene.lute"),
        "{v}"
    );
    assert!(
        warns[0]["message"]
            .as_str()
            .unwrap()
            .contains("entry.nope.read"),
        "{v}"
    );
}

// --- dsl 0.20.0 fact envelopes ------------------------------------------------

const FACT_VOCAB: &str = "entities:\n  crew: { members: [vesna, toma] }\n  \
    topic: { members: [heading, manifest] }\nrelations:\n  \
    knows: { args: [crew, topic], tier: run }\n";

/// The verified gap: a line guard over a ground fact asserted nowhere (the
/// relation IS asserted, with other arguments) is `E-ARM-DEAD` under
/// `check-project` and stays silent under single-file `check`, which cannot
/// see sibling asserts.
#[test]
fn check_project_flags_a_line_guard_over_a_never_asserted_fact() {
    let dir = temp_dir("fact-envelope-line-guard");
    write(
        &dir,
        "archive.lute",
        &format!(
            "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: 1\n{FACT_VOCAB}---\n\
             ## Shot 1.\n::assert{{knows(vesna, manifest)}}\n@vesna: noted.\n"
        ),
    );
    let bridge = write(
        &dir,
        "bridge.lute",
        &format!(
            "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: 2\n{FACT_VOCAB}---\n\
             ## Shot 1.\n@vesna{{when=\"holds('knows', ['vesna', 'manifest'])\"}}: Two pods.\n\
             @vesna{{when=\"holds('knows', ['toma', 'heading'])\"}}: So you read the log.\n"
        ),
    );

    let single = run(&["check", bridge.to_str().unwrap(), "--json"]);
    assert_eq!(
        single.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&single.stdout)
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diags = v["project_diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1, "only the never-asserted guard: {v}");
    assert_eq!(diags[0]["code"], "E-ARM-DEAD", "{v}");
    assert!(
        diags[0]["path"].as_str().unwrap().ends_with("bridge.lute"),
        "{v}"
    );
    assert!(
        diags[0]["message"]
            .as_str()
            .unwrap()
            .contains("no seed, assert, rule, or engine relation produces `holds('knows', ['toma', 'heading'])`"),
        "{v}"
    );
}

/// A quest whose required objective is dead only at the argument level
/// (the relation is asserted, the queried fact never is) is unreachable to
/// complete, so a scene gated on `completed(Q)` is `E-CONN-UNREACHABLE` —
/// connectivity and the diagnostic read the same verdict.
#[test]
fn argument_level_dead_objective_marks_completed_gate_unreachable() {
    let dir = temp_dir("fact-envelope-completed-gate");
    write(
        &dir,
        "q.lute",
        &format!(
            "---\nkind: quest\n{FACT_VOCAB}---\n<quest id=\"q\" start=\"true\">\n\
             <objective id=\"o\" done=\"holds('knows', ['toma', 'heading'])\"/>\n</quest>\n"
        ),
    );
    write(
        &dir,
        "a.lute",
        &format!(
            "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: 1\n{FACT_VOCAB}---\n\
             ## Shot 1.\n::assert{{knows(toma, manifest)}}\n@vesna: hi.\n"
        ),
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: gated\nseason: 1\nepisode: 1\n\
         after: 'completed(\"q\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes: Vec<&str> = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == "E-OBJECTIVE-UNSATISFIABLE")
            .count(),
        1,
        "{v}"
    );
    assert!(codes.contains(&"E-CONN-UNREACHABLE"), "{v}");
}

/// dsl 0.20.0 §5 removes `W-UNPROVEN-RELATIONAL` (the 0.10.0 `W-INJECT-
/// CONFLICT` precedent): the code leaves the deny registry, so naming it is a
/// usage error rather than a promotion that silently protects nothing.
#[test]
fn deny_of_the_removed_unproven_relational_code_is_a_usage_error() {
    let dir = temp_dir("deny-removed-unproven");
    let out = run(&[
        "check-project",
        dir.to_str().unwrap(),
        "--deny",
        "W-UNPROVEN-RELATIONAL",
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ok = run(&[
        "check-project",
        dir.to_str().unwrap(),
        "--deny",
        "W-FACT-GUARANTEED",
    ]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

// --- dsl 0.23.0 §10: `check-project --wip` ----------------------------------

/// A lore document whose entry reacts to `found(toma)` — a relation nothing
/// produces yet — plus, when `with_dead_match`, an entry on `knows(toma,
/// heading)`, a relation that IS asserted but never with those arguments.
fn wip_lore(with_dead_match: bool) -> String {
    let extra = if with_dead_match {
        "<entry id=\"heading\" when=\"holds('knows', ['toma', 'heading'])\">\n  @vesna: The heading.\n</entry>\n\
         <entry id=\"note\">\n  @vesna: Noted.\n  ::assert{knows(toma, manifest)}\n</entry>\n"
    } else {
        ""
    };
    format!(
        "---\nkind: lore\ntitle: Records\nentities:\n  crew: {{ members: [vesna, toma] }}\n  \
         topic: {{ members: [heading, manifest] }}\nrelations:\n  found: {{ args: [crew], tier: run }}\n  \
         knows: {{ args: [crew, topic], tier: run }}\n---\n\
         <entry id=\"found\" when=\"holds('found', ['toma'])\">\n  @vesna: Found him.\n</entry>\n{extra}"
    )
}

#[test]
fn wip_downgrades_a_guard_dead_only_for_want_of_a_producer() {
    let dir = temp_dir("wip-unwritten");
    write(&dir, "notes.lute", &wip_lore(false));
    let plain = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&plain.stdout);
    assert_eq!(plain.status.code(), Some(1), "{text}");
    assert!(text.contains("error [E-ENTRY-UNREACHABLE]"), "{text}");

    let wip = run(&["check-project", "--wip", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&wip.stdout);
    assert_eq!(wip.status.code(), Some(0), "{text}");
    // The downgrade prints as its own warning code, never `warning [E-…]`;
    // the message names the error it is without the flag.
    assert!(text.contains("warning [W-WIP] entry `found`"), "{text}");
    assert!(
        text.contains("`E-ENTRY-UNREACHABLE` without `--wip`"),
        "{text}"
    );
    assert!(!text.contains("warning [E-"), "{text}");
}

#[test]
fn wip_keeps_a_produced_relation_that_never_matches_an_error() {
    let dir = temp_dir("wip-never-matches");
    write(&dir, "notes.lute", &wip_lore(true));
    let wip = run(&["check-project", "--wip", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&wip.stdout);
    assert_eq!(wip.status.code(), Some(1), "{text}");
    assert!(text.contains("warning [W-WIP] entry `found`"), "{text}");
    assert!(
        text.contains("error [E-ENTRY-UNREACHABLE] entry `heading`"),
        "{text}"
    );
}

/// A beat on an occasion, in a project with a `terminal:`, dead only for want
/// of a producer is judged twice — by its `when` alone and under
/// `!terminal` — but reported once, with `--wip` as without.
#[test]
fn wip_reports_a_beat_dead_under_the_terminal_once() {
    let dir = temp_dir("wip-terminal-once");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { p: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions:\n  chime: { select: first }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  crew: { members: [vesna, toma] }\nrelations:\n  found: { args: [crew], tier: run }\n\
         state:\n  run.over: { type: bool, default: false }\nterminal: \"run.over\"\n",
    );
    write(
        &dir,
        "notes.lute",
        "---\nkind: lore\nid: notes\ntitle: Records\n---\n\
         <entry id=\"found\" on=\"chime\" when=\"holds('found', ['toma'])\">\n  @narrator: Found him.\n</entry>\n",
    );
    for (flag, code) in [(None, "E-ENTRY-UNREACHABLE"), (Some("--wip"), "W-WIP")] {
        let mut args = vec!["check-project"];
        args.extend(flag);
        args.push(dir.to_str().unwrap());
        let out = run(&args);
        let text = String::from_utf8_lossy(&out.stdout);
        let dead = text
            .lines()
            .filter(|l| l.contains(&format!("[{code}] entry `found`")))
            .count();
        assert_eq!(dead, 1, "{text}");
    }
}

/// An id `add:`ed to a second kind is `E-ENTITY-KIND-CLASH` at that `add:`
/// entry — the later declaration — naming both places relative to the
/// project root, not at the kind that declared the id first.
#[test]
fn entity_kind_clash_is_reported_at_the_add_naming_both_files() {
    let dir = temp_dir("kind-clash-at-add");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml, schema/isles.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  island: { members: [coralreach, mistral] }\n  person: { members: [tamsin] }\n",
    );
    write(
        &dir,
        "schema/isles.schema.yaml",
        "entities:\n  person:\n    add:\n      - coralreach\n",
    );
    write(
        &dir,
        "scenes/s.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = text
        .lines()
        .find(|l| l.contains("[E-ENTITY-KIND-CLASH]"))
        .unwrap_or_else(|| panic!("no E-ENTITY-KIND-CLASH:\n{text}"));
    assert!(
        line.contains("isles.schema.yaml:4:9: error"),
        "at the `add:` entry: {line}"
    );
    assert!(
        line.contains("`island` (`world.schema.yaml:2`)")
            && line.contains("`person` (`schema/isles.schema.yaml:4`)"),
        "both places, relative: {line}"
    );
}

/// A document's own `uses:` replaces `defaults.uses`: an error naming
/// something only the replaced list declared says so and names the schema;
/// a name no schema declares gets no such note.
#[test]
fn replaced_default_uses_is_named_on_the_errors_it_causes() {
    let dir = temp_dir("defaults-uses-replaced");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.world: { type: int, default: 0 }\n",
    );
    write(
        &dir,
        "wake.schema.yaml",
        "state:\n  run.wake: { type: int, default: 0 }\n",
    );
    write(
        &dir,
        "scenes/own.lute",
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: [../wake.schema.yaml]\n---\n\
         \n## S\n\n::set{ run.world = 1 }\n::set{ run.nowhere = 1 }\n@a: hi\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = |path: &str| {
        text.lines()
            .find(|l| l.contains("[E-UNDECLARED]") && l.contains(path))
            .unwrap_or_else(|| panic!("no E-UNDECLARED for {path}:\n{text}"))
    };
    assert!(
        line("run.world").contains(
            "this document's `uses:` replaces `defaults.uses` (from lute.project.yaml), and \
             `run.world` is declared in `world.schema.yaml`"
        ),
        "{text}"
    );
    assert!(!line("run.nowhere").contains("defaults.uses"), "{text}");
}

/// A document's own `components:` replaces `defaults.components`: a
/// `<beat use>` naming a component only the replaced list imports says so
/// and names its file; an unknown name keeps the ordinary hint.
#[test]
fn replaced_default_components_is_named_on_the_template_use() {
    let dir = temp_dir("defaults-components-replaced");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
         defaults:\n  components: [components/bond.lute]\n",
    );
    write(
        &dir,
        "components/bond.lute",
        "---\ncomponent: bondStory\nparams:\n  who: { type: string }\n\
         beat:\n  on: bond\n---\n## Bond\n::body\n@narrator: The bond deepens.\n",
    );
    write(
        &dir,
        "components/greet.lute",
        "---\ncomponent: greet\n---\n## G\n@narrator: Hi there.\n",
    );
    write(
        &dir,
        "lore/bonds.lute",
        "---\nkind: lore\nid: bonds\ncomponents: [../components/greet.lute]\n---\n\n\
         <beat use=\"bondStory\" id=\"first\" who=\"aria\">\n@narrator: Hello.\n</beat>\n\n\
         <beat use=\"nowhereStory\" id=\"second\" who=\"aria\">\n@narrator: Hello.\n</beat>\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let line = |name: &str| {
        text.lines()
            .find(|l| l.contains("[E-TEMPLATE]") && l.contains(&format!("use=\"{name}\"")))
            .unwrap_or_else(|| panic!("no E-TEMPLATE for {name}:\n{text}"))
    };
    assert!(
        line("bondStory").contains(
            "this document's `components:` replaces `defaults.components` (from \
             lute.project.yaml), which imports it from `components/bond.lute`"
        ),
        "{text}"
    );
    assert!(
        !line("nowhereStory").contains("defaults.components"),
        "{text}"
    );
}

fn branch_scene(branch: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n\
         <branch id=\"{branch}\">\n<choice id=\"c\" text=\"L\">\n@a: yo\n</choice>\n</branch>\n"
    )
}

/// Two documents declaring one `<branch id>`: a play's `choose:` key answers
/// both, so the later one is warned, naming the other relative to the root.
/// Distinct ids say nothing; a repeat inside one document stays its own
/// `E-DUP-BRANCH` and is not also this warning.
#[test]
fn check_project_warns_on_a_branch_id_two_documents_share() {
    let dir = temp_dir("branch-id-shared");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "scenes/a.lute", &branch_scene("dig"));
    write(&dir, "scenes/b.lute", &branch_scene("dig"));
    write(&dir, "scenes/c.lute", &branch_scene("other"));
    write(
        &dir,
        "scenes/d.lute",
        &format!(
            "{}<branch id=\"twice\">\n<choice id=\"c\" text=\"L\">\n@a: yo\n</choice>\n</branch>\n",
            branch_scene("twice")
        ),
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let shared: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("[W-BRANCH-ID-SHARED]"))
        .collect();
    assert_eq!(shared.len(), 1, "{text}");
    assert!(
        shared[0].contains("scenes/b.lute:11:1: warning")
            && shared[0].contains("at `scenes/a.lute:11`")
            && shared[0].contains("`choose: { dig: … }` answers both"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("[E-DUP-BRANCH]") && l.contains("scenes/d.lute")),
        "{text}"
    );
}

/// Causes print first, in the order a fix must follow: the manifest's rows,
/// then the schemas', then each document in path order, then the rows about
/// documents the project as a whole finds.
#[test]
fn check_project_prints_causes_before_documents() {
    let dir = temp_dir("cause-order");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  \
         luteVersion: \"0.20.0\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.mood: { type: { enum: [calm, very calm] }, default: calm }\n",
    );
    write(
        &dir,
        "scenes/b.lute",
        "---\nkind: quest\nid: b\n---\n<quest id=\"waits\" title=\"Waits\" tier=\"run\">\n  \
         <objective id=\"o\" title=\"O\" done=\"run.mood == 'calm'\"/>\n</quest>\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n## A\n<branch id=\"pick one\" prompt=\"?\">\n  \
         <choice id=\"x\" text=\"X\">\n    @n: x\n  </choice>\n</branch>\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let at = |needle: &str| {
        stdout
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` missing from:\n{stdout}"))
    };
    let order = [
        at("lute.project.yaml:6:3: warning [W-LUTE-VERSION-STALE]"),
        at("world.schema.yaml:2:3: error [E-PATH-IDENT] enum member `very calm`"),
        at("a.lute:6:13: error [E-PATH-IDENT] branch id `pick one`"),
        at("failed: "),
        at("b.lute (0 warning(s))"),
        at("project-wide diagnostics:"),
        at("b.lute:5:12: warning [W-QUEST-NEVER-ACCEPTED]"),
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}\n{stdout}");
}
/// dsl 0.31.0 §3/§4: required objective clock-window diagnostics use the
/// calendar position and distinguish a reset-per-run info from a contention
/// warning. The deadline variant is the clean counterpart.
#[test]
fn check_project_reports_sera_kato_contention_and_stranded_info() {
    let dir = temp_dir("objective-clock-windows");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"0.31.0\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
         run.slot: { type: { enum: [morning, evening] }, default: evening, owner: engine }\n  \
         run.sera: { type: bool, default: false }\n  run.kato: { type: bool, default: false }\n\
         clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, evening]\n  raise: tick\n  raiseAtStart: true\n",
    );
    write(
        &dir,
        "quests/sera-kato.lute",
        "---\nkind: quest\nid: seraKato\n---\n<quest id=\"seraKato\" tier=\"run\" start=\"true\">\n\
         <objective id=\"sera\" done=\"run.sera\"/>\n\
         <objective id=\"kato\" done=\"run.kato\"/>\n</quest>\n",
    );
    write(
        &dir,
        "scenes/sera.lute",
        "---\nkind: scene\nid: sera\non: tick\nwhen: \"clock.index == 1\"\npriority: 10\nadvances: slot\n---\n\
         ## Sera\n@narrator: Sera takes the evening watch.\n::set{run.sera = true}\n",
    );
    write(
        &dir,
        "scenes/kato.lute",
        "---\nkind: scene\nid: kato\non: tick\nwhen: \"clock.index == 1\"\npriority: 9\nadvances: slot\n---\n\
         ## Kato\n@narrator: Kato takes the evening watch.\n::set{run.kato = true}\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    let stranded: Vec<_> = text
        .lines()
        .filter(|line| line.contains("[W-OBJECTIVE-STRANDED]"))
        .collect();
    assert_eq!(stranded.len(), 2, "{text}");
    assert!(
        stranded.iter().all(|line| line.contains("info")),
        "{text}"
    );
    let contention: Vec<_> = text
        .lines()
        .filter(|line| line.contains("[W-SLOT-CONTENTION]"))
        .collect();
    assert_eq!(contention.len(), 2, "{text}");
    assert!(contention.iter().all(|line| line.contains("day 1 evening")), "{text}");

    let clean = temp_dir("objective-clock-deadline");
    write(
        &clean,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"0.31.0\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &clean,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
         run.slot: { type: { enum: [morning, evening] }, default: evening, owner: engine }\n  \
         run.done: { type: bool, default: false }\nclock:\n  day: run.day\n  slot: run.slot\n  \
         slots: [morning, evening]\n  raise: tick\n  raiseAtStart: true\n",
    );
    write(
        &clean,
        "quests/main.lute",
        "---\nkind: quest\nid: main\n---\n<quest id=\"main\" tier=\"user\" start=\"true\">\n\
         <objective id=\"done\" done=\"run.done\" until=\"clock.index >= 2\"/>\n\
         <on event=\"questFailed\">@narrator: missed.</on>\n</quest>\n",
    );
    write(
        &clean,
        "scenes/main.lute",
        "---\nkind: scene\nid: main\non: tick\nwhen: \"clock.index == 1\"\nadvances: slot\n---\n\
         ## Main\n@narrator: Main.\n::set{run.done = true}\n",
    );
    let clean_out = run(&["check-project", clean.to_str().unwrap()]);
    let clean_text = String::from_utf8_lossy(&clean_out.stdout);
    assert!(
        !clean_text.contains("W-OBJECTIVE-STRANDED"),
        "{clean_text}"
    );
}

/// An unguarded repeatable beat on the clock's own raise must be rejected
/// before runtime can recurse through an endless declared-advance cascade.
#[test]
fn check_project_rejects_repeatable_advance_cascade() {
    let dir = temp_dir("advance-cascade");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"0.31.0\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n\
         run.slot: { type: { enum: [morning, evening] }, default: morning, owner: engine }\n\
         clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, evening]\n  raise: tick\n",
    );
    write(
        &dir,
        "scenes/loop.lute",
        "---\nkind: scene\nid: loop\non: tick\nonce: false\nadvances: slot\n---\n\
         ## Loop\n@narrator: Loop.\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("[E-ADVANCE-CASCADE]"), "{text}");
    assert!(text.contains("scene `loop`"), "{text}");
}

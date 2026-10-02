//! The 0.28 clock edges through the built `lute` binary: the slot path of a
//! clock that ends on its first day is narrowed to the slots it reaches; a
//! clock whose day outlives the run keeps its spends across `newRun`;
//! `clock.ended` turns true at the advance that ends the clock; a relation's
//! `tier: season:<name>` resets when the season opens; a component param
//! typed `{ domain: clock.weekdayLabel }` is checked against the clock.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-clock028-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn check_project(dir: &Path) -> Output {
    Command::new(BIN)
        .arg("check-project")
        .arg(dir)
        .output()
        .unwrap()
}

fn play(dir: &Path, script: &str) -> Output {
    write(dir, "s.play.yaml", script);
    Command::new(BIN)
        .args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"))
        .output()
        .unwrap()
}

/// A project whose schema is `schema` and whose plugin `p` declares
/// `occasions` (a YAML mapping body), plus `docs`.
fn project(tag: &str, schema: &str, occasions: &str, docs: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: p\nprofiles: { p: { plugins: { p: true } } }\n\
         defaults: { uses: [world.schema.yaml] }\n",
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
        &format!("occasions:\n{occasions}"),
    );
    write(&dir, "world.schema.yaml", schema);
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    dir
}

/// hollow-ward's one-night clock, ending at `last_slot` on night 1.
fn ward_schema(last_slot: &str) -> String {
    format!(
        "state:\n  run.night: {{ type: int, default: 1, owner: engine }}\n  \
         run.hour: {{ type: {{ enum: [h23, h00, h01, h02, h03, h04, h05] }}, default: h23, owner: engine }}\n\
         clock:\n  day: run.night\n  slot: run.hour\n  slots: [h23, h00, h01, h02, h03, h04, h05]\n  \
         raise: {{ slot: hourStrikes, dayEnd: dawn }}\n{last_slot}"
    )
}

fn scene(id: &str, on: &str, head: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\non: {on}\n{head}---\n\n## S\n\n{body}\n")
}

/// A clock that ends at h03 on its first night never reaches h04 or h05:
/// the slot path (and `clock.slot`) holds only h23..h03, so a `when` on a
/// later slot is unreachable, an arm for one is dead, and a `<match>` needs
/// no arm for them. The same content on a clock that never ends is clean.
#[test]
fn a_one_day_clock_narrows_its_slot_path_to_the_slots_it_reaches() {
    let arms = |slots: &[&str]| {
        let mut s = String::from("<match on=\"run.hour\">\n");
        for slot in slots {
            s.push_str(&format!(
                "  <when is=\"{slot}\">\n    @narrator: {slot}.\n  </when>\n"
            ));
        }
        s.push_str("</match>");
        s
    };
    let reached = ["h23", "h00", "h01", "h02", "h03"];
    let all = ["h23", "h00", "h01", "h02", "h03", "h04", "h05"];
    let docs = [
        (
            "scenes/late.lute",
            scene(
                "late",
                "hourStrikes",
                "when: \"run.hour == 'h05'\"\npriority: 10\n",
                "@narrator: Five.",
            ),
        ),
        (
            "scenes/lateslot.lute",
            scene(
                "lateslot",
                "hourStrikes",
                "when: \"clock.slot == 'h04'\"\npriority: 20\n",
                "@narrator: Four.",
            ),
        ),
        (
            "scenes/strike.lute",
            scene("strike", "hourStrikes", "once: false\n", &arms(&all)),
        ),
        (
            "scenes/short.lute",
            scene(
                "short",
                "hourStrikes",
                "once: false\npriority: 5\n",
                &arms(&reached),
            ),
        ),
        (
            "scenes/gated.lute",
            scene(
                "gated",
                "hourStrikes",
                "once: false\npriority: 1\n",
                "@narrator{when=\"run.hour == 'h05'\"}: Five.",
            ),
        ),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let dir = project(
        "narrow",
        &ward_schema("  last: { day: 1, slot: h03 }\n"),
        "  hourStrikes: { select: first }\n  dawn: { select: first }\n",
        &docs,
    );
    let out = check_project(&dir);
    let t = text(&out);
    let holds =
        "the clock ends at its last position, so `run.hour` only holds h23, h00, h01, h02, h03";
    assert!(
        t.contains(&format!(
            "[E-BEAT-UNREACHABLE] beat `late` is never eligible: its `when` `run.hour == 'h05'` is provably false — {holds}"
        )),
        "{t}"
    );
    assert!(
        t.contains("[E-BEAT-UNREACHABLE] beat `lateslot` is never eligible"),
        "`clock.slot` narrows with the slot path: {t}"
    );
    assert!(
        t.contains(&format!(
            "[E-ARM-DEAD] arm can never fire: its pattern `h04` never comes — {holds}"
        )),
        "{t}"
    );
    assert!(t.contains("its pattern `h05` never comes"), "{t}");
    assert!(
        t.contains(&format!(
            "[E-ARM-DEAD] this gated line can never be shown: its `when` guard is provably false — {holds}"
        )),
        "{t}"
    );
    assert!(
        !t.contains("short.lute:"),
        "a match over the reached slots is exhaustive: {t}"
    );

    // The same content on a clock that never ends: every slot comes round.
    let dir = project(
        "narrow-open",
        &ward_schema(""),
        "  hourStrikes: { select: first }\n  dawn: { select: first }\n",
        &docs,
    );
    let t = text(&check_project(&dir));
    assert!(!t.contains("E-BEAT-UNREACHABLE"), "{t}");
    assert!(!t.contains("E-ARM-DEAD"), "{t}");
    assert!(
        t.contains("[E-NONEXHAUSTIVE]") && t.contains("short.lute:"),
        "{t}"
    );
}

const DIVE_OCCASIONS: &str = "  hub: { select: first }\n";

fn dive(tag: &str, day: &str) -> PathBuf {
    let schema = format!(
        "state:\n  {day}: {{ type: int, default: 1, owner: engine }}\n\
         clock:\n  day: {day}\n  week: {{ length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }}\n"
    );
    let docs = [
        (
            "scenes/deck.lute",
            scene("deck", "hub", "once: false\n", "@narrator: The deck."),
        ),
        (
            "scenes/stew.lute",
            scene(
                "stew",
                "hub",
                "once: week\npriority: 10\n",
                "@narrator: {{clock.weekdayLabel}} stew.",
            ),
        ),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    project(tag, &schema, DIVE_OCCASIONS, &docs)
}

/// The Drowned Crown's calendar: a clock whose day is `user.dive` outlives
/// the run, so a `once: week` beat spent on Monday stays spent on Tuesday
/// of the next run, and the new-run step says the clock is kept. A run-tier
/// clock starts over, and its spends go with it.
#[test]
fn a_user_tier_clock_keeps_its_spends_across_a_new_run() {
    let dir = dive("user", "user.dive");
    let out = play(
        &dir,
        "steps:\n  - occasion: hub\n    expect: { winner: stew }\n  \
         - newRun: { state: { user.dive: 2 } }\n    expect: { clock: { weekday: Tue } }\n  \
         - occasion: hub\n    expect: { winner: deck }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains(
            "  the clock is kept: its day `user.dive` outlives the run, so its position, its once: \
             day / slot / week spends stay\n"
        ),
        "{t}"
    );
    assert!(
        t.contains("once: week — already presented this week"),
        "{t}"
    );

    let dir = dive("run", "run.dive");
    let out = play(
        &dir,
        "steps:\n  - occasion: hub\n    expect: { winner: stew }\n  - newRun: true\n  \
         - occasion: hub\n    expect: { winner: stew }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("the clock is kept"), "{t}");
}

/// `clock.ended` turns true in the settle of the advance that ends the
/// clock: the dawn raise of that advance sees it, a `by="clock.ended"`
/// deadline fails there and not one slot early, and `expect.clock.ended`
/// judges it. On a clock that never ends `expect.clock.ended` is a usage
/// error.
#[test]
fn clock_ended_turns_true_at_the_advance_that_ends_the_clock() {
    let quest = "---\nkind: quest\nid: q\n---\n\n\
                 <quest id=\"escape\" title=\"Out\" start=\"true\" tier=\"run\">\n  \
                 <objective id=\"out\" title=\"Leave\" done=\"visited('exit')\" by=\"clock.ended\"/>\n\
                 </quest>\n";
    let docs = [
        (
            "scenes/strike.lute",
            scene(
                "strike",
                "hourStrikes",
                "once: false\n",
                "@narrator: Strike.",
            ),
        ),
        (
            "scenes/dawn.lute",
            scene(
                "dawn",
                "dawn",
                "when: \"clock.ended\"\n",
                "@narrator: The ward is closed.",
            ),
        ),
        (
            "scenes/exit.lute",
            scene("exit", "leave", "", "@narrator: Out."),
        ),
        ("quests/q.lute", quest.to_string()),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let occasions =
        "  hourStrikes: { select: first }\n  dawn: { select: first }\n  leave: { select: first }\n";
    let dir = project(
        "ended",
        &ward_schema("  last: { day: 1, slot: h05 }\n"),
        occasions,
        &docs,
    );
    let out = play(
        &dir,
        "steps:\n  - advance: 6\n    expect: { clock: { slot: h05, ended: false }, quests: { escape: active } }\n  \
         - advance: slot\n    expect: { clock: { ended: true }, presented: [dawn], quests: { escape: failed } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("The ward is closed."), "{t}");

    // A wrong expectation names the key and the actual value.
    let out = play(
        &dir,
        "steps:\n  - advance: 6\n    expect: { clock: { ended: true } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "expect clock ended: expected true, actual false — the clock stands at its last \
             position (day 1 h05) and ends on the next `advance:`"
        ),
        "{t}"
    );

    let dir = project("ended-open", &ward_schema(""), occasions, &docs[..1]);
    let out = play(
        &dir,
        "steps:\n  - advance: slot\n    expect: { clock: { ended: false } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains(
            "`expect.clock.ended` — the clock never ends (it declares no `last:` or `days:`)"
        ),
        "{t}"
    );
}

/// `clock.ended` turns true before the last `dayEnd` is raised, so with
/// `terminal: "clock.ended"` the game is over and that raise is closed: a
/// beat only it would take never plays. The checker warns, the calendar
/// shows the cell `not raised`, and play agrees. Without the terminal the
/// raise is made, and nothing is warned. A `dayStart` beat for the day the
/// run starts is warned too — the clock does not raise it where the run
/// starts — unless the clock declares `raiseAtStart: true` (the engine
/// raises it there): then nothing is warned and the calendar shows it raised.
#[test]
fn a_beat_only_a_raise_the_clock_never_makes_would_take_is_warned() {
    let clock_schema = |terminal: &str, extra: &str| {
        format!(
            "state:\n  run.day: {{ type: int, default: 1, owner: engine }}\n  \
             run.slot: {{ type: {{ enum: [dawn, dusk] }}, default: dawn, owner: engine }}\n\
             {terminal}clock:\n  day: run.day\n  slot: run.slot\n  slots: [dawn, dusk]\n  \
             raise: {{ dayStart: morning, dayEnd: nightfall }}\n  last: {{ day: 2 }}\n{extra}"
        )
    };
    let schema = |terminal: &str| clock_schema(terminal, "");
    let docs = [(
        "scenes/farewell.lute",
        scene(
            "farewell",
            "nightfall",
            "when: \"clock.day == 2\"\n",
            "@narrator: The boats sail home.",
        ),
    )];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let occasions = "  nightfall: { select: first }\n  morning: { select: first }\n";
    let steps = "steps:\n  - advance: day\n  - advance: day\n";

    let dir = project(
        "closed-end",
        &schema("terminal: \"clock.ended\"\n"),
        occasions,
        &docs,
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains(
            "warning [W-BEAT-UNRAISED] scene `farewell` answers `nightfall`, but its `when` \
             `clock.day == 2` holds at no raise of it: it can hold at day 2 dusk, where only the \
             advance that ends the clock raises `nightfall`"
        ),
        "{t}"
    );
    let cal = Command::new(BIN)
        .args([
            "calendar",
            &dir.display().to_string(),
            "--axis",
            "clock=1..2",
        ])
        .args(["--occasion", "nightfall"])
        .output()
        .unwrap();
    let c = text(&cal);
    assert!(cal.status.success(), "{c}");
    assert!(
        c.lines()
            .any(|l| l.starts_with("2 dusk") && l.contains("not raised")),
        "{c}"
    );
    let t = text(&play(&dir, steps));
    assert!(!t.contains("The boats sail home."), "{t}");

    let dir = project("open-end", &schema(""), occasions, &docs);
    let t = text(&check_project(&dir));
    assert!(!t.contains("W-BEAT-UNRAISED"), "{t}");
    let t = text(&play(&dir, steps));
    assert!(t.contains("The boats sail home."), "{t}");

    let first = [(
        "scenes/arrival.lute",
        scene(
            "arrival",
            "morning",
            "when: \"run.day == 1\"\n",
            "@narrator: Off the bus.",
        ),
    )];
    let first: Vec<(&str, &str)> = first.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let dir = project("first-day", &schema(""), occasions, &first);
    let t = text(&check_project(&dir));
    assert!(
        t.contains(
            "[W-BEAT-UNRAISED] scene `arrival` answers `morning`, but its `when` `run.day == 1` \
             holds at no raise the clock makes: the clock does not raise `morning` at day 1 \
             dawn, where the run starts — if the engine raises it when a run starts, declare \
             `raiseAtStart: true` on the clock; otherwise answer an occasion raised where it \
             holds"
        ),
        "{t}"
    );
    assert!(!t.contains("never plays"), "{t}");
    let calendar = |dir: &Path| {
        let cal = Command::new(BIN)
            .args([
                "calendar",
                &dir.display().to_string(),
                "--axis",
                "clock=1..2",
            ])
            .args(["--occasion", "morning"])
            .output()
            .unwrap();
        let c = text(&cal);
        assert!(cal.status.success(), "{c}");
        c
    };
    let start_cell = |c: &str| {
        c.lines()
            .find(|l| l.starts_with("1 dawn"))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("{c}"))
    };
    let c = calendar(&dir);
    assert!(start_cell(&c).contains("not raised"), "{c}");

    let dir = project(
        "first-day-raised",
        &clock_schema("", "  raiseAtStart: true\n"),
        occasions,
        &first,
    );
    let t = text(&check_project(&dir));
    assert!(!t.contains("W-BEAT-UNRAISED"), "{t}");
    let c = calendar(&dir);
    let cell = start_cell(&c);
    assert!(
        !cell.contains("not raised") && cell.contains("arrival"),
        "{c}"
    );
    // Only where the run starts: day 1's dusk is still not raised.
    assert!(
        c.lines()
            .any(|l| l.starts_with("1 dusk") && l.contains("not raised")),
        "{c}"
    );
}

/// A relation with `tier: season:<name>` goes back to the seed facts each
/// time the season opens (and the play says so); naming an undeclared
/// season is `E-SEASON-DECL` with a did-you-mean.
#[test]
fn a_season_tier_relation_resets_when_its_season_opens() {
    let schema = "state:\n  user.fest: { type: bool, default: false, owner: engine }\n\
                  entities:\n  crew: { members: [mira, ren] }\n\
                  relations:\n  wished: { args: [crew], tier: \"season:lanterns\" }\n\
                  seasons:\n  lanterns: { live: \"user.fest\" }\n";
    let wish = "<match>\n  <when test=\"holds('wished', ['mira'])\">\n    @narrator: Already wished.\n  </when>\n  \
                <otherwise>\n    @narrator: A wish.\n    ::assert{wished(mira)}\n  </otherwise>\n</match>";
    let docs = [(
        "scenes/wish.lute",
        scene("wish", "hub", "once: false\n", wish),
    )];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let dir = project("season", schema, DIVE_OCCASIONS, &docs);
    let out = play(
        &dir,
        "steps:\n  - engine: { state: { user.fest: true } }\n  - occasion: hub\n  \
         - occasion: hub\n    expect: { facts: [wished(mira)] }\n  \
         - engine: { state: { user.fest: false } }\n  \
         - engine: { state: { user.fest: true } }\n    expect: { notFacts: [wished(mira)] }\n  \
         - occasion: hub\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("season lanterns opens — season.lanterns.* reset to defaults, wished facts back to their seed facts"),
        "{t}"
    );
    assert_eq!(t.matches("@narrator: A wish.").count(), 2, "{t}");
    assert_eq!(t.matches("@narrator: Already wished.").count(), 1, "{t}");

    let typo = schema.replace("tier: \"season:lanterns\"", "tier: \"season:lantern\"");
    let dir = project("season-typo", &typo, DIVE_OCCASIONS, &docs);
    let t = text(&check_project(&dir));
    assert!(
        t.contains("[E-SEASON-DECL] relation `wished`'s `tier: season:lantern` names season `lantern`, which no schema declares (declared: lanterns) — did you mean `lanterns`?"),
        "{t}"
    );

    // A season spelt as a state path or bare, or a state path for a tier, is
    // refused at its `tier:` key with the tier it means.
    for (tier, meant) in [
        ("season.lanterns", "season:lanterns"),
        ("lanterns", "season:lanterns"),
        ("run.wished", "run"),
    ] {
        let bad = schema.replace("tier: \"season:lanterns\"", &format!("tier: {tier}"));
        let dir = project("season-spelling", &bad, DIVE_OCCASIONS, &docs);
        let t = text(&check_project(&dir));
        let line = t
            .lines()
            .find(|l| l.contains("[E-RELATION-DOMAIN]"))
            .unwrap_or_else(|| panic!("{t}"));
        assert!(
            line.contains("world.schema.yaml:6:27: error")
                && line.contains(&format!("`tier: {tier}` — did you mean `{meant}`?")),
            "{t}"
        );
    }

    // An undeclared season in `defaults.questTier` is the manifest's, reported
    // once there — not at each quest it applies to.
    let quests = "---\nkind: quest\nid: q.doc\n---\n\n\
                  <quest id=\"a\" title=\"A\" start=\"true\" done=\"holds('wished', ['mira'])\"/>\n\
                  <quest id=\"b\" title=\"B\" start=\"true\" done=\"holds('wished', ['ren'])\"/>\n";
    let mut docs = docs.clone();
    docs.push(("quests/q.lute", quests));
    let dir = project("season-quest-tier", schema, DIVE_OCCASIONS, &docs);
    let manifest = std::fs::read_to_string(dir.join("lute.project.yaml")).unwrap();
    write(
        &dir,
        "lute.project.yaml",
        &manifest.replace(
            "defaults: { uses: [world.schema.yaml] }",
            "defaults:\n  uses: [world.schema.yaml]\n  questTier: season:lantern\n",
        ),
    );
    let t = text(&check_project(&dir));
    let errors: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[E-SEASON-DECL]"))
        .collect();
    assert_eq!(errors.len(), 1, "{t}");
    assert!(
        errors[0].contains("lute.project.yaml:6:3: error")
            && errors[0].contains("`defaults.questTier: season:lantern` names season `lantern`"),
        "{t}"
    );
}

/// A component param typed `{ domain: clock.weekdayLabel }` takes the
/// clock's labels as its members: the body's `<match>` is judged against
/// them (a misspelt label, a missing one) without copying the list.
#[test]
fn a_component_param_can_name_the_clock_s_weekday_labels() {
    let schema = "state:\n  run.day: { type: int, default: 1, owner: engine }\n\
                  clock:\n  day: run.day\n  week: { length: 3, first: 0, labels: [Mon, Tue, Wed] }\n\
                  defs:\n  today: \"clock.weekdayLabel\"\n";
    let card = |arms: &str| {
        format!(
            "---\ncomponent: dayCard\nparams:\n  day: {{ domain: clock.weekdayLabel }}\n---\n\n\
             ## Day card\n\n<match on=\"@day\">\n{arms}</match>\n"
        )
    };
    let arm =
        |label: &str| format!("  <when is=\"{label}\">\n    @narrator: {label}.\n  </when>\n");
    let host = scene(
        "morning",
        "hub",
        "once: false\ncomponents: [../components/day-card.component.lute]\n",
        "::use{component=\"dayCard\" day=@today}\n::use{component=\"dayCard\" day=\"Tue\"}",
    );
    let good = card(&format!("{}{}{}", arm("Mon"), arm("Tue"), arm("Wed")));
    let docs = [
        ("scenes/morning.lute", host.clone()),
        ("components/day-card.component.lute", good),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let dir = project("card", schema, DIVE_OCCASIONS, &docs);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));

    let bad = card(&format!("{}{}", arm("Mon"), arm("Wednesdy")));
    write(&dir, "components/day-card.component.lute", &bad);
    let t = text(&check_project(&dir));
    assert!(
        t.contains("[E-WHEN-LITERAL-DOMAIN] `Wednesdy` is not a member of the subject's domain [Mon, Tue, Wed]"),
        "{t}"
    );
    assert!(
        t.contains("[E-NONEXHAUSTIVE]") && t.contains("`Tue`"),
        "{t}"
    );
    assert!(
        !t.contains("W-DOMAIN-UNREAD"),
        "the clock's enums are not declarations: {t}"
    );
}

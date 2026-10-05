use super::*;

/// Human report: one block per test with per-expectation pass/fail lines on a
/// miss, then a `N passed, M failed` summary and (optional) coverage.
pub(super) fn render_human(
    dir: &Path,
    results: &[TestResult],
    cov: Option<(&CoverageAccum, &Path)>,
    units: &[CoverageUnit],
) -> String {
    let mut out = String::new();
    let out = &mut out;
    if results.is_empty() {
        outln!(
            out,
            "no *.test.yaml files (or *.play.yaml files carrying `expect:`) under {}",
            dir.display()
        );
    }
    for r in results {
        let mark = if r.passed { "PASS" } else { "FAIL" };
        // T3-23: a test that never resolved a document names none — no `()`.
        let what = match (r.kind == "play", r.lute_file.is_empty()) {
            (_, true) => String::new(),
            (true, false) => format!("  (play of {})", r.lute_file),
            (false, false) => format!("  ({})", r.lute_file),
        };
        outln!(out, "{mark}  {}{what}", r.test_file.display());
        if let Some(lines) = &r.refusal {
            // The vector carries either the trace's own held diagnostics
            // (#25) or the harness's own `E-TEST-*` refusals (#2). Only the
            // first is a *trace* refusal, so only it gets that header.
            if r.exit == "refused" {
                outln!(out, "      trace refused:");
            }
            for line in lines {
                outln!(out, "        {line}");
            }
            for a in &r.autopicked {
                outln!(out, "      auto-picked (no selection supplied): {a}");
            }
            continue;
        }
        // A halted play: the halt is the root cause, so it comes first, and
        // the end-of-play expectations it left unmet fold into one line.
        let halted = r.kind == "play" && !r.notes.is_empty();
        if halted {
            for n in &r.notes {
                outln!(out, "      halted: {n}");
            }
        }
        if !r.passed {
            // OT-F-10: a false eligibility is the cause of the state and
            // fact misses its unwalked body leaves, so it comes first.
            let (causes, rest): (Vec<&ExpectResult>, Vec<&ExpectResult>) = r
                .expectations
                .iter()
                .filter(|e| !e.passed)
                .partition(|e| e.kind == "eligible");
            for e in causes.into_iter().chain(rest) {
                render_miss(out, e);
            }
            let (at_end, misses): (Vec<&ExpectMiss>, Vec<&ExpectMiss>) = r
                .misses
                .iter()
                .partition(|m| halted && m.step.is_none() && m.key != "end");
            for m in misses {
                outln!(out, "      {m}");
            }
            match at_end.as_slice() {
                [] => {}
                [m] => outln!(out, "      {m}"),
                ms => {
                    outln!(
                    out,
                    "      end of play: {} expectations missed on the world the halt left ({}) \
                     — fix the halt first",
                    ms.len(),
                    ms.iter().map(|m| m.key.as_str()).collect::<Vec<_>>().join(", ")
                )
                }
            }
        }
        // T3-11: WHY the walk stopped (or left a guard undecided), with the
        // test keys that would decide it — the failure used to say only
        // `expected complete, got incomplete`.
        for u in &r.unresolved {
            outln!(
                out,
                "      unresolved: {} `{}` ({}) — supply {} in this test",
                u.construct,
                u.expression,
                u.id,
                yaml_atom_hints(u)
            );
        }
        for u in &r.forced_unknown {
            let hints = yaml_atom_hints(u);
            outln!(
                out,
                "      unresolved (forced): {} `{}` was chosen past a guard that was unknown \
                 (`{}`){}",
                u.construct,
                u.id,
                u.expression,
                if hints.is_empty() {
                    String::new()
                } else {
                    format!(" — supply {hints} to decide it")
                }
            );
        }
        if !halted {
            for n in &r.notes {
                outln!(out, "      note: {n}");
            }
        }
        for a in &r.autopicked {
            outln!(out, "      auto-picked (no selection supplied): {a}");
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    outln!(out, "\n{passed} passed, {failed} failed");

    if let Some((cov, root)) = cov {
        render_coverage_human(out, cov, root, units);
    }
    std::mem::take(out)
}

/// One failed expectation's miss line(s).
fn render_miss(out: &mut String, e: &ExpectResult) {
    match (e.kind, e.actual.as_deref()) {
        ("transcriptContains", Some(actual)) => outln!(
            out,
            "      transcriptContains {:?}: {actual} (expected present)",
            e.expected
        ),
        ("transcriptLacks", Some(actual)) => outln!(
            out,
            "      transcriptLacks {:?}: {actual} (expected absent)",
            e.expected
        ),
        ("options", Some(actual)) => outln!(
            out,
            "      options {}: expected {}, got {actual}",
            e.subject,
            e.expected
        ),
        ("options", None) => outln!(
            out,
            "      options {}: expected {}, but the walk never presented a branch/hub `{}`",
            e.subject,
            e.expected,
            e.subject
        ),
        ("state", Some(actual)) => outln!(
            out,
            "      state {}: expected {}, got {}",
            e.subject,
            state_literal(&e.expected),
            state_literal(actual)
        ),
        ("quests", _) if !QUEST_STATES.contains(&e.expected.as_str()) => outln!(
            out,
            "      quests {}: {:?} is not a quest state (expected one of: {})",
            e.subject,
            e.expected,
            QUEST_STATES.join(", ")
        ),
        ("quests", Some(actual)) => {
            outln!(
                out,
                "      quests {}: expected {:?}, got {:?}",
                e.subject,
                e.expected,
                actual
            );
            if let Some(why) = &e.why {
                outln!(out, "      note: {why}");
            }
        }
        ("quests", None) => outln!(
            out,
            "      quests {}: expected {:?}, but {}",
            e.subject,
            e.expected,
            e.why
                .as_deref()
                .unwrap_or("the traced document declares no such quest")
        ),
        // T9.9: there is no observed value to print. The old line printed
        // the sentinel on the `got` side, so a test whose expected literal
        // happened to BE the sentinel's text rendered `expected "<never
        // written>", got "<never written>"` — a difference whose two sides
        // were byte identical. Since T2-5 "no value" means never written,
        // not seeded, and no declared `default:`.
        ("state", None) => {
            outln!(
                out,
                "      state {}: expected {}, but the path was never written and has no seed \
                 or declared default",
                e.subject,
                state_literal(&e.expected)
            );
            if e.expected == NEVER_WRITTEN {
                // Do not invent grammar here: `expect:`'s key set is closed
                // and the new expectation kinds are deferred with #19 (D-B).
                // Name the gap instead.
                outln!(
                    out,
                    "      note: {NEVER_WRITTEN:?} is how this report DISPLAYS an absent value, \
                     not a literal an expectation can match; `expect:` has no \"never written\" \
                     form (legal keys: {}) — deferred with #19",
                    TEST_EXPECT_KEYS.join(", ")
                );
            }
        }
        ("end", Some(actual)) if e.subject == IMPLICIT_EXIT => outln!(
            out,
            "      end: {actual} — an unknown guard halted the walk before the end, so the \
             expectations after it were never walked; an incomplete trace fails unless the \
             test declares `expect: {{ end: incomplete }}`"
        ),
        ("end", Some(actual)) => outln!(out, "      end: expected {}, got {actual}", e.expected),
        ("facts" | "notFacts", Some(actual)) => outln!(
            out,
            "      {} {}: expected {}, got {actual}",
            e.kind,
            e.subject,
            e.expected
        ),
        ("eligible", Some(_)) if e.expected == IMPLICIT_ELIGIBLE => outln!(
            out,
            "      eligible {id}: not eligible under these mocks ({}) — the engine would never \
             present it, so the walk proves nothing about play; fix the mocks, or assert \
             `expect: {{ eligible: {{ {id}: false }} }}` (the body is then not walked)",
            e.why.as_deref().unwrap_or("its `when` is false"),
            id = e.subject,
        ),
        ("eligible", Some(actual)) => outln!(
            out,
            "      eligible{}: expected {}, got {actual}{}",
            if e.subject.is_empty() {
                String::new()
            } else {
                format!(" {}", e.subject)
            },
            e.expected,
            e.why
                .as_deref()
                .map(|why| format!(" — {why}"))
                .unwrap_or_default()
        ),
        ("eligible", None) if e.subject.is_empty() => outln!(
            out,
            "      eligible: expected {}, but the test presented no entry, beat or scene",
            e.expected
        ),
        ("eligible", None) => outln!(
            out,
            "      eligible {}: expected {}, but the document declares no such entry or beat{}",
            e.subject,
            e.expected,
            e.why
                .as_deref()
                .map(|hint| format!(" — {hint}"))
                .unwrap_or_default()
        ),
        ("accepts", Some(actual)) => {
            outln!(out, "      accepts: expected {}, got {actual}", e.expected)
        }
        _ => {}
    }
}

/// A state value as a miss line shows it: numbers and booleans bare (`3`,
/// `true`), anything else quoted (`"open"`) — the text form carries no type,
/// so the shape decides.
fn state_literal(text: &str) -> String {
    if text == "true" || text == "false" || text.parse::<f64>().is_ok_and(f64::is_finite) {
        text.to_string()
    } else {
        format!("{text:?}")
    }
}

/// Human coverage view — honest header, chosen/never-chosen names where the
/// reports expose them, counts where they do not. Every row names its
/// construct's own file and site; the guard text rides along as a label
/// (#24, T9.13). `units` are the TESTABLE units ([`coverage_units`]) —
/// components are not in it, and the strings below say so.
fn render_coverage_human(
    out: &mut String,
    cov: &CoverageAccum,
    root: &Path,
    units: &[CoverageUnit],
) {
    let root = display_path(root);
    if cov.plays == 0 {
        outln!(out, "\ncoverage over {} traced path(s):", cov.paths);
    } else {
        // T3-20: a play feeds the units it presented and the branch/hub
        // picks it made; the arm rows below are the traced paths' alone
        // (lighthouse N3).
        outln!(
            out,
            "\ncoverage over {} traced path(s) and {} play(s) (plays count toward what they \
             presented and the choices they picked, not match arms):",
            cov.paths,
            cov.plays
        );
    }
    if cov.choices.is_empty() && cov.arms.is_empty() {
        outln!(out, "  (no branch/hub or match constructs traced)");
    }
    for row in cov.choices.values() {
        let (chosen, total) = (&row.chosen, row.total);
        let never_named: Vec<&String> = row.eligible.difference(chosen).collect();
        let mut line = format!(
            "  branch/hub {} ({}): {}/{} chosen",
            row.label,
            row.site,
            chosen.len().min(total),
            total
        );
        if !chosen.is_empty() {
            line.push_str(&format!(
                " [{}]",
                chosen.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !never_named.is_empty() {
            line.push_str(&format!(
                "; never chosen [{}]",
                never_named
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        // Choices never seen eligible anywhere: count only, honest.
        let unseen = total.saturating_sub(chosen.len() + never_named.len());
        if unseen > 0 {
            let anywhere = if cov.plays == 0 {
                "any traced path"
            } else {
                "any traced path or play"
            };
            line.push_str(&format!("; {unseen} never seen eligible in {anywhere}"));
        }
        outln!(out, "{line}");
    }
    for (key, row) in &cov.arms {
        let (label, chosen, total) = (&row.label, &row.chosen, &row.total);
        if row.guard {
            // T3-22: a `when=` guard is taken or skipped, not a match.
            let (taken, skipped) = (
                chosen.contains(lute_trace::report::GUARD_TAKEN),
                chosen.contains(lute_trace::report::GUARD_SKIPPED),
            );
            let seen = match (taken, skipped) {
                (true, true) => "taken and skipped",
                (true, false) => "taken; never skipped",
                (false, true) => "skipped; never taken",
                (false, false) => "never decided",
            };
            outln!(out, "  guard `{label}` ({key}): {seen}");
            continue;
        }
        let unexecuted = total.saturating_sub(chosen.len());
        // A `<match>` with no `on` has no subject to quote (ML-F4).
        let what = if label.trim().is_empty() {
            "match with no subject".to_string()
        } else {
            format!("match `{label}`")
        };
        let mut line = format!(
            "  {what} ({key}): {}/{} arm(s) executed",
            chosen.len().min(*total),
            total
        );
        if !chosen.is_empty() {
            line.push_str(&format!(
                " [{}]",
                chosen.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if unexecuted > 0 {
            line.push_str(&format!("; {unexecuted} unexecuted"));
        }
        outln!(out, "{line}");
    }
    // T9.13's real design hole: coverage accumulated only from reports that
    // RAN, so deleting a test made its scene invisible rather than untested.
    // T3-20: and a lore document holding two endings stayed "named" while
    // one of them lost its every proof — the unit is the beat, not the file.
    // Both strings say "testable", because component documents are out of the
    // denominator and claiming otherwise is the false-reassurance this whole
    // task is about.
    let untested: Vec<&CoverageUnit> = units.iter().filter(|u| !u.covered(cov)).collect();
    if untested.is_empty() {
        outln!(
            out,
            "  every testable document, beat and entry under {} is presented by at least one \
             test or play",
            root
        );
    } else {
        outln!(
            out,
            "  {} untested unit(s) under {} — no *.test.yaml presents them and no play presents \
             them:",
            untested.len(),
            root
        );
        for line in grouped(&untested) {
            outln!(out, "    {line}");
        }
    }
    // T3-20: the ending-proof view. A scene test traces a beat from mocked
    // state; only a play reaches it from the start, so a beat no play
    // presented has no proof it is reachable in play.
    let beats: Vec<&CoverageUnit> = units.iter().filter(|u| u.beat).collect();
    if beats.is_empty() {
        return;
    }
    if cov.plays == 0 {
        outln!(
            out,
            "  no play ran, so none of the {} beat(s) under {} is proven presented in play",
            beats.len(),
            root
        );
        return;
    }
    // OT-F-14: an untested beat is unplayed too; it is listed once, above.
    let unplayed: Vec<&CoverageUnit> = beats
        .into_iter()
        .filter(|u| !u.presented_by_play(cov))
        .collect();
    let traced_only: Vec<&CoverageUnit> = unplayed
        .iter()
        .copied()
        .filter(|u| u.covered(cov))
        .collect();
    if unplayed.is_empty() {
        outln!(out, "  every beat under {} is presented by a play", root);
    } else if traced_only.is_empty() {
        outln!(
            out,
            "  every other beat under {} is presented by a play",
            root
        );
    } else {
        outln!(
            out,
            "  {} {}beat(s) no play presents (a test traces them; only a play proves they are \
             reached in play):",
            traced_only.len(),
            if traced_only.len() < unplayed.len() {
                "more "
            } else {
                ""
            }
        );
        for line in grouped(&traced_only) {
            outln!(out, "    {line}");
        }
    }
}

/// Units one line per document, in walk order: `scenes/x.lute` for a whole
/// document, `lore/endings/ren.lute: ember, lantern` for its beats/entries.
fn grouped(units: &[&CoverageUnit]) -> Vec<String> {
    let mut lines: Vec<(String, Vec<&str>)> = Vec::new();
    for u in units {
        if lines.last().is_none_or(|(f, _)| *f != u.file) {
            lines.push((u.file.clone(), Vec::new()));
        }
        if u.id.is_some() {
            lines.last_mut().expect("pushed").1.push(&u.local);
        }
    }
    lines
        .into_iter()
        .map(|(f, ids)| {
            if ids.is_empty() {
                f
            } else {
                format!("{f}: {}", ids.join(", "))
            }
        })
        .collect()
}

/// One unit as JSON: its file, its project id (an entry's global id, a
/// bundle beat's `<doc>.<beat>`, a scene's key), the document it is in
/// (`doc`), its id there (`local`, `null` for a whole document), and its
/// kind.
fn unit_json(u: &CoverageUnit) -> serde_json::Value {
    serde_json::json!({
        "file": u.file,
        "id": u.id.clone().or_else(|| (!u.local.is_empty()).then(|| u.local.clone())),
        "doc": u.doc,
        "local": u.id.as_ref().map(|_| u.local.clone()),
        "kind": u.kind,
    })
}

/// One unresolved entry as JSON (T3-11): where it is, what was undecided,
/// the raw atoms, and the test keys that would decide it.
fn unresolved_json(u: &UnresolvedEntry) -> serde_json::Value {
    serde_json::json!({
        "construct": u.construct,
        "id": u.id,
        "line": u.span.line,
        "column": u.span.column,
        "expression": u.expression,
        "atoms": u.atoms,
        "supply": u.atoms.iter().map(|a| yaml_atom_hint(a)).collect::<Vec<_>>(),
    })
}

/// Machine report: per-test verdicts + expectations, the summary, and
/// (optional) coverage — stable-keyed JSON.
pub(super) fn render_json(
    results: &[TestResult],
    cov: Option<(&CoverageAccum, &Path)>,
    units: &[CoverageUnit],
) -> String {
    use serde_json::{json, Value};

    let tests: Vec<Value> = results
        .iter()
        .map(|r| {
            let expectations: Vec<Value> = r
                .expectations
                .iter()
                .map(|e| {
                    let mut v = json!({
                        "kind": e.kind,
                        "subject": e.subject,
                        "expected": e.expected,
                        "actual": e.actual,
                        "passed": e.passed,
                        "evidence": "witnessed",
                        "script": r.test_file.display().to_string(),
                    });
                    // dsl 0.27.0 §4 (HW27-04): an `eligible` miss because
                    // the engine would not raise the beat's occasion.
                    if let Some(nr) = e.not_raised.as_ref().filter(|_| !e.passed) {
                        v["notRaised"] = json!(nr);
                    }
                    v
                })
                .collect();
            json!({
                "test": r.test_file.display().to_string(),
                "kind": r.kind,
                "file": r.lute_file,
                "script": r.test_file.display().to_string(),
                "exit": r.exit,
                "end": r.end,
                "passed": r.passed,
                "evidence": if r.passed { "witnessed" } else { "unknown" },
                "refusal": r.refusal,
                "autopicked": r.autopicked.clone(),
                "expectations": expectations,
                "misses": r.misses.iter().map(ExpectMiss::to_json).collect::<Vec<_>>(),
                "unresolved": r.unresolved.iter().map(unresolved_json).collect::<Vec<_>>(),
                "forcedUnknown": r.forced_unknown.iter().map(unresolved_json).collect::<Vec<_>>(),
                "notes": r.notes,
            })
        })
        .collect();

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    let mut root = json!({
        "tests": tests,
        "summary": { "passed": passed, "failed": failed },
    });

    if let Some((cov, cov_root)) = cov {
        // Keyed by the printed site (the first spelling seen), as before.
        let choices: serde_json::Map<String, Value> = cov
            .choices
            .values()
            .map(|row| {
                let (chosen, total) = (&row.chosen, row.total);
                let never_named: Vec<&String> = row.eligible.difference(chosen).collect();
                let unseen = total.saturating_sub(chosen.len() + never_named.len());
                (
                    row.site.clone(),
                    json!({
                        "label": row.label,
                        "total": total,
                        "chosen": chosen.iter().cloned().collect::<Vec<_>>(),
                        "neverChosen": never_named.iter().map(|s| (*s).clone()).collect::<Vec<_>>(),
                        "neverEligibleInAnyPath": unseen,
                    }),
                )
            })
            .collect();
        let arms: serde_json::Map<String, Value> = cov
            .arms
            .iter()
            .map(|(key, row)| {
                (
                    key.clone(),
                    json!({
                        "kind": if row.guard { "guard" } else { "match" },
                        "label": row.label,
                        "total": row.total,
                        "executed": row.chosen.iter().cloned().collect::<Vec<_>>(),
                        "unexecuted": row.total.saturating_sub(row.chosen.len()),
                    }),
                )
            })
            .collect();
        root["coverage"] = json!({
            "tracedPaths": cov.paths,
            "plays": cov.plays,
            "choices": Value::Object(choices),
            "arms": Value::Object(arms),
            "root": display_path(cov_root),
            // T3-20: the units (whole documents, bundle beats, entries) no
            // test and no play presented.
            "untested": units
                .iter()
                .filter(|u| !u.covered(cov))
                .map(unit_json)
                .collect::<Vec<_>>(),
            // T3-20: the beats (anything answering an occasion) a test
            // traces but no play presented — an untested beat is listed
            // once, under `untested`, as the text does (OT-F-14).
            "notPresentedByPlay": units
                .iter()
                .filter(|u| u.beat && u.covered(cov) && !u.presented_by_play(cov))
                .map(unit_json)
                .collect::<Vec<_>>(),
        });
    }

    format!(
        "{}\n",
        serde_json::to_string_pretty(&root).expect("report is JSON-serializable")
    )
}

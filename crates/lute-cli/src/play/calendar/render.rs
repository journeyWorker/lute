use super::*;


/// Which beats' unknown `when` leaves a cell undecided, in words
/// (inn.oldFriend's `when` is unknown).
pub(super) fn undecided_why(unknown: &[(String, String)]) -> String {
    let ids: Vec<&str> = unknown.iter().map(|(id, _)| id.as_str()).collect();
    match ids.as_slice() {
        [one] => format!("{one}'s `when` is unknown"),
        many => format!("the `when` of {} is unknown", many.join(", ")),
    }
}

/// The grid's cell text: the winner (or the offered list), `?` when an
/// unknown `when` decides it, `not raised` where the clock's `raise:` map
/// never raises the occasion, `gate false` when the occasion's `raisedWhen`
/// is false there, `-` when nothing is presented; `+N` counts the eligible
/// beats it shadows.
pub(super) fn cell_text(o: &Outcome) -> String {
    let mut s = if o.not_raised {
        "not raised".to_string()
    } else if o.undecided {
        "?".to_string()
    } else if o.presented.is_empty() && o.gated {
        "gate false".to_string()
    } else if o.presented.is_empty() {
        "-".to_string()
    } else {
        o.presented.join(", ")
    };
    if !o.shadowed.is_empty() {
        let _ = write!(s, " +{}", o.shadowed.len());
    }
    s
}

pub(super) fn cell_label(cell: &Cell) -> String {
    cell.at
        .iter()
        .map(|(path, text, _)| format!("{path}={text}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A cell's values alone, `/`-joined — a presence table's column head.
pub(super) fn cell_short(cell: &Cell) -> String {
    if cell.at.is_empty() {
        return "(start)".to_string();
    }
    cell.at
        .iter()
        .map(|(_, text, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// `occasion[@target]` of a beat.
pub(super) fn beat_on(b: &IndexBeat) -> String {
    match &b.target {
        Some(t) => format!("{}@{t}", b.on),
        None => b.on.clone(),
    }
}

/// Left-aligned columns two spaces apart; trailing blanks trimmed.
pub(super) fn table(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    let widths: Vec<usize> = (0..rows.first().map_or(0, Vec::len))
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0))
        .collect();
    for row in rows {
        let mut line = String::new();
        for (i, v) in row.iter().enumerate() {
            if i + 1 == row.len() {
                line.push_str(v);
            } else {
                let _ = write!(line, "{v:<w$}  ", w = widths[i]);
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// `(varies, held)` of a per-occasion column: the axes (or clock parts) it
/// varies over and `(path, value text, value)` of every one it is held at.
pub(super) fn pinned(pins: &[Pin], axes: &[Axis]) -> (Vec<String>, Vec<(String, String, Value)>) {
    let mut varies = Vec::new();
    let mut held = Vec::new();
    for (pin, axis) in pins.iter().zip(axes) {
        match pin {
            Pin::Varies => varies.push(axis.path.clone()),
            Pin::At(k) => {
                let (text, value) = &axis.values[*k];
                held.push((axis.path.clone(), text.clone(), value.clone()));
            }
            Pin::Clock {
                day, slot, slotted, ..
            } => {
                match day {
                    None => varies.push("clock.day".to_string()),
                    Some(d) => held.push((
                        "clock.day".to_string(),
                        d.to_string(),
                        Value::Int(*d),
                    )),
                }
                match slot {
                    None if !slotted => {}
                    None => varies.push("clock.slot".to_string()),
                    Some(s) => {
                        held.push(("clock.slot".to_string(), s.clone(), Value::Str(s.clone())))
                    }
                }
            }
        }
    }
    (varies, held)
}

/// A presence table's rows: per first argument (the closed domain's
/// members, then any other first argument some cell holds), its text at
/// every cell — the remaining arguments, `yes` for a unary relation, `-`
/// when nothing holds. A nullary relation is one row under its name.
pub(super) fn fact_rows(ri: usize, rel: &FactsRel, cells: &[Cell]) -> Vec<(String, Vec<String>)> {
    if rel.args.is_empty() {
        let row = cells
            .iter()
            .map(|c| if c.facts[ri].is_empty() { "-" } else { "yes" }.to_string())
            .collect();
        return vec![(rel.name.clone(), row)];
    }
    let mut firsts = rel.members.clone();
    let seen: BTreeSet<&str> = cells
        .iter()
        .flat_map(|c| c.facts[ri].iter().map(|(_, args)| args[0].as_str()))
        .filter(|a| !rel.members.iter().any(|m| m == a))
        .collect();
    firsts.extend(seen.into_iter().map(str::to_string));
    firsts
        .into_iter()
        .map(|first| {
            let row = cells
                .iter()
                .map(|c| {
                    let here: Vec<String> = c.facts[ri]
                        .iter()
                        .filter(|(_, args)| args[0] == first)
                        .map(|(_, args)| {
                            if args.len() == 1 {
                                "yes".to_string()
                            } else {
                                args[1..].join(", ")
                            }
                        })
                        .collect();
                    if here.is_empty() {
                        "-".to_string()
                    } else {
                        here.join(" | ")
                    }
                })
                .collect();
            (first, row)
        })
        .collect()
}

pub(super) fn render_text(dir: &Path, r: &Report<'_>) -> String {
    let (axes, columns, cells) = (r.axes, r.columns, r.cells);
    let mut out = String::new();
    let pruned = match r.pruned {
        0 => String::new(),
        n => format!(" ({n} dropped by --where)"),
    };
    let _ = writeln!(
        out,
        "calendar: {} — {} cell(s){pruned} × {} column(s), from {}",
        dir.display(),
        cells.len(),
        columns.len(),
        r.from
    );
    let mut noted = BTreeSet::new();
    for c in columns {
        let Some(pins) = &c.pins else { continue };
        if !noted.insert(c.occasion.as_str()) {
            continue;
        }
        let (varies, held) = pinned(pins, axes);
        let varies = if varies.is_empty() {
            "no axis".to_string()
        } else {
            varies.join(", ")
        };
        let held: Vec<String> = held.iter().map(|(p, t, _)| format!("{p}={t}")).collect();
        let held = if held.is_empty() {
            String::new()
        } else {
            format!(", at {}", held.join(" "))
        };
        let _ = writeln!(
            out,
            "  {}: varies over {varies} only{held}; blank elsewhere",
            c.occasion
        );
    }
    for (_, why, elsewhere) in r.unraised {
        if *elsewhere {
            let _ = writeln!(out, "  {why}; `not raised` elsewhere");
        } else {
            let _ = writeln!(out, "  {why}");
        }
    }
    out.push('\n');
    // Two header rows: the axis paths and occasions, then the targets.
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(cells.len() + 2);
    let targeted = columns.iter().any(|c| c.target.is_some() || c.any_target);
    let mut head: Vec<String> = axes
        .iter()
        .map(|a| crate::output::spelled_path(&a.path))
        .collect();
    head.extend(columns.iter().map(|c| {
        if c.select == OccasionSelect::First {
            c.occasion.clone()
        } else {
            format!("{} ({})", c.occasion, c.select.as_str())
        }
    }));
    rows.push(head);
    if targeted {
        let mut second: Vec<String> = axes.iter().map(|_| String::new()).collect();
        second.extend(columns.iter().map(|c| match &c.target {
            Some(t) => t.clone(),
            None if c.any_target => "(any)".to_string(),
            None => String::new(),
        }));
        rows.push(second);
    }
    for cell in cells {
        let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
        row.extend(
            cell.outcomes
                .iter()
                .map(|o| o.as_ref().map(cell_text).unwrap_or_default()),
        );
        rows.push(row);
    }
    out.push_str(&table(&rows));

    for (ri, rel) in r.rels.iter().enumerate() {
        let _ = writeln!(out, "\nfacts {}({}):", rel.name, rel.args.join(", "));
        let first = rel.args.first().unwrap_or(&rel.name).clone();
        let mut rows = vec![std::iter::once(first)
            .chain(cells.iter().map(cell_short))
            .collect::<Vec<_>>()];
        rows.extend(
            fact_rows(ri, rel, cells)
                .into_iter()
                .map(|(label, row)| std::iter::once(label).chain(row).collect()),
        );
        out.push_str(&table(&rows));
    }

    let mut shadowed = String::new();
    let mut undecided = String::new();
    let mut notes = String::new();
    let mut payloads: BTreeSet<String> = BTreeSet::new();
    for cell in cells {
        let label = cell_label(cell);
        for n in &cell.notes {
            let _ = writeln!(notes, "  {label}: {n}");
        }
        for (col, o) in columns.iter().zip(&cell.outcomes) {
            let Some(o) = o else { continue };
            if !o.shadowed.is_empty() {
                // An undecided cell presents nothing: the eligible beats wait
                // behind the unknown `when`, which `?` stands for.
                let over = if o.undecided {
                    format!("undecided ({})", undecided_why(&o.unknown))
                } else {
                    o.presented.join(", ")
                };
                let _ = writeln!(
                    shadowed,
                    "  {label}  {}: {over} over {}",
                    col.label(),
                    o.shadowed.join(", ")
                );
            }
            if o.undecided {
                for (id, why) in &o.unknown {
                    let _ = writeln!(undecided, "  {label}  {}: {id} — {why}", col.label());
                    // A payload field no axis gives a value.
                    let mut rest = why.as_str();
                    while let Some(i) = rest.find("`occasion.payload.") {
                        let field = &rest[i + 1..];
                        let Some(end) = field.find('`') else { break };
                        let path = &field[..end];
                        if path
                            .chars()
                            .all(|c| c.is_alphanumeric() || "._".contains(c))
                        {
                            payloads.insert(path.to_string());
                        }
                        rest = &field[end..];
                    }
                }
            }
        }
    }
    if !payloads.is_empty() {
        let axes: Vec<String> = payloads
            .iter()
            .map(|f| format!("`--axis {f}=<value>,…`"))
            .collect();
        let _ = writeln!(
            undecided,
            "  no axis gives the raise a payload: vary it with {}",
            axes.join(" and ")
        );
    }
    for (title, body) in [
        ("shadowed (eligible, not presented):", shadowed),
        (
            "undecided (an unknown `when` decides the cell; play halts there):",
            undecided,
        ),
        ("notes:", notes),
    ] {
        if !body.is_empty() {
            let _ = write!(out, "\n{title}\n{body}");
        }
    }
    for (title, list, detail) in [
        ("never eligible in any cell", &r.never_eligible, "" as &str),
        (
            "eligible but never presented in any cell",
            &r.never_presented,
            "lost to ",
        ),
    ] {
        let title = format!("{title} [bounded: {}]", r.scope);
        let _ = write!(out, "\n{title}: ");
        if list.is_empty() {
            out.push_str("none\n");
            continue;
        }
        let _ = writeln!(out, "{}", list.len());
        for (b, s) in list {
            let why: Vec<&str> = if detail.is_empty() {
                &s.reasons
            } else {
                &s.beaten_by
            }
            .iter()
            .map(String::as_str)
            .collect();
            let _ = writeln!(
                out,
                "  {} [{}, {}] {} — {detail}{}",
                b.id,
                kind_label(b.kind),
                b.document,
                beat_on(b),
                why.join("; ")
            );
        }
    }
    out
}

pub(super) fn render_json(r: &Report<'_>) -> Json {
    let col_json = |c: &Column| {
        let mut m = serde_json::Map::new();
        m.insert("occasion".into(), json!(c.occasion));
        if let Some(t) = &c.target {
            m.insert("target".into(), json!(t));
        }
        if c.any_target {
            m.insert("anyTarget".into(), json!(true));
        }
        m.insert("select".into(), json!(c.select.as_str()));
        m
    };
    let beat_json = |b: &IndexBeat| {
        let mut m = serde_json::Map::new();
        m.insert("id".into(), json!(b.id));
        m.insert("kind".into(), json!(kind_label(b.kind)));
        m.insert("document".into(), json!(b.document));
        m.insert("on".into(), json!(b.on));
        if let Some(t) = &b.target {
            m.insert("target".into(), json!(t));
        }
        m
    };
    let scope = r.scope.clone();
    let mut out = json!({
        "from": r.from,
        "scope": r.scope,
        "evidence": "bounded",
        "pruned": r.pruned,
        "axes": r.axes.iter().map(|a| json!({
            "path": a.path,
            "values": a.values.iter().map(|(_, v)| value_to_json(v)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "columns": r.columns.iter().map(|c| {
            let mut m = col_json(c);
            if let Some(pins) = &c.pins {
                let (varies, held) = pinned(pins, r.axes);
                m.insert("varies".into(), json!(varies));
                m.insert("heldAt".into(), Json::Object(
                    held.iter().map(|(p, _, v)| (p.to_string(), value_to_json(v))).collect(),
                ));
            }
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "cells": r.cells.iter().map(|cell| {
            let at: serde_json::Map<String, Json> =
                cell.at.iter().map(|(p, _, v)| (p.clone(), v.clone())).collect();
            let mut m = serde_json::Map::new();
            m.insert("at".into(), Json::Object(at));
            if !cell.notes.is_empty() {
                m.insert("notes".into(), json!(cell.notes));
            }
            m.insert("results".into(), Json::Array(r.columns.iter().zip(&cell.outcomes).filter_map(|(c, o)| {
                let o = o.as_ref()?;
                let mut r = col_json(c);
                r.insert("winner".into(), json!(o.winner));
                r.insert("presented".into(), json!(o.presented));
                r.insert("shadowed".into(), json!(o.shadowed));
                if o.undecided {
                    r.insert("evidence".into(), json!("unknown"));
                } else {
                    r.insert("evidence".into(), json!("bounded"));
                    r.insert("witnessed".into(), json!(true));
                    r.insert("scope".into(), json!(scope.clone()));
                }
                if !o.unknown.is_empty() {
                    r.insert("unknown".into(), Json::Array(o.unknown.iter().map(|(id, why)| {
                        json!({ "id": id, "reason": why })
                    }).collect()));
                }
                if o.undecided {
                    r.insert("undecided".into(), json!(true));
                }
                if o.gated {
                    r.insert("gated".into(), json!(true));
                }
                if o.not_raised {
                    r.insert("notRaised".into(), json!(true));
                }
                Some(Json::Object(r))
            }).collect()));
            if !r.rels.is_empty() {
                m.insert("facts".into(), Json::Object(r.rels.iter().zip(&cell.facts).map(|(rel, fs)| {
                    (rel.name.clone(), json!(fs.iter().map(render_fact).collect::<Vec<_>>()))
                }).collect()));
            }
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "neverEligible": r.never_eligible.iter().map(|(b, s)| {
            let mut m = beat_json(b);
            m.insert("reasons".into(), json!(s.reasons));
            m.insert("evidence".into(), json!("bounded"));
            m.insert("scope".into(), json!(scope.clone()));
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "neverPresented": r.never_presented.iter().map(|(b, s)| {
            let mut m = beat_json(b);
            m.insert("beatenBy".into(), json!(s.beaten_by));
            m.insert("evidence".into(), json!("bounded"));
            m.insert("scope".into(), json!(scope.clone()));
            Json::Object(m)
        }).collect::<Vec<_>>(),
    });
    // With `--axis clock`: where the clock raises each column shown
    // `notRaised` in some cell.
    if !r.unraised.is_empty() {
        out["notRaised"] = Json::Array(
            r.unraised
                .iter()
                .map(|(o, why, _)| json!({ "occasion": o, "reason": why }))
                .collect(),
        );
    }
    out
}

/// RFC 4180 field: quoted when it holds a comma, quote or newline.
pub(super) fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub(super) fn csv_line(out: &mut String, fields: &[String]) {
    let _ = writeln!(
        out,
        "{}",
        fields
            .iter()
            .map(|f| csv_field(f))
            .collect::<Vec<_>>()
            .join(",")
    );
}

/// One row per cell × evaluated column (a cell no column is evaluated at
/// still gets one row when `--facts` asks for its facts); list fields
/// `;`-joined, a `facts:<relation>` field per `--facts`. The beats eligible
/// somewhere but never presented follow as a second table after a blank
/// line, when there are any.
pub(super) fn render_csv(r: &Report<'_>) -> String {
    let mut out = String::new();
    let mut head: Vec<String> = r.axes.iter().map(|a| a.path.clone()).collect();
    head.extend(
        [
            "occasion",
            "target",
            "select",
            "winner",
            "presented",
            "shadowed",
            "unknown",
            "notes",
        ]
        .map(str::to_string),
    );
    head.extend(r.rels.iter().map(|rel| format!("facts:{}", rel.name)));
    csv_line(&mut out, &head);
    for cell in r.cells {
        let facts: Vec<String> = cell
            .facts
            .iter()
            .map(|fs| fs.iter().map(render_fact).collect::<Vec<_>>().join(";"))
            .collect();
        let mut rows: Vec<[String; 8]> = Vec::new();
        for (c, o) in r.columns.iter().zip(&cell.outcomes) {
            let Some(o) = o else { continue };
            let unknown: Vec<&str> = o.unknown.iter().map(|(id, _)| id.as_str()).collect();
            rows.push([
                c.occasion.clone(),
                c.target.clone().unwrap_or_else(|| {
                    if c.any_target {
                        "(any)".to_string()
                    } else {
                        String::new()
                    }
                }),
                c.select.as_str().to_string(),
                o.winner.clone().unwrap_or_default(),
                o.presented.join(";"),
                o.shadowed.join(";"),
                unknown.join(";"),
                o.not_raised
                    .then(|| "not raised".to_string())
                    .into_iter()
                    .chain(o.gated.then(|| "gate false".to_string()))
                    .chain(cell.notes.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(";"),
            ]);
        }
        if rows.is_empty() && !r.rels.is_empty() {
            let mut bare: [String; 8] = Default::default();
            bare[7] = cell.notes.join(";");
            rows.push(bare);
        }
        for fields in rows {
            let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
            row.extend(fields);
            row.extend(facts.iter().cloned());
            csv_line(&mut out, &row);
        }
    }
    if !r.never_presented.is_empty() {
        out.push('\n');
        csv_line(
            &mut out,
            &[
                "neverPresented",
                "kind",
                "document",
                "occasion",
                "target",
                "beatenBy",
            ]
            .map(str::to_string),
        );
        for (b, s) in &r.never_presented {
            csv_line(
                &mut out,
                &[
                    b.id.clone(),
                    kind_label(b.kind).to_string(),
                    b.document.clone(),
                    b.on.clone(),
                    b.target.clone().unwrap_or_default(),
                    s.beaten_by.iter().cloned().collect::<Vec<_>>().join(";"),
                ],
            );
        }
    }
    out
}

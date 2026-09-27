//! Reserved names (dsl 0.28.0 §1): the names the language keeps for itself,
//! in one table.
//!
//! A reserved word used to be accepted where it was declared and misread
//! where it was used — an entity member `clock` read as the state root, an
//! enum member `unset` never matched by `is="unset"`, a choice id `true` read
//! as the boolean. Every declaration check asks [`refusal`] and refuses the
//! name at its declaration, naming one to use instead. `lute --explain
//! E-RESERVED-NAME` prints this table ([`render_text`]) and the website's
//! reference page lists it ([`render_markdown`], pinned by a `lute-cli`
//! test), so the checks, the terminal and the docs cannot drift apart.

use crate::snapshot::BUILTIN_LIFECYCLE_EVENTS;

/// A place a name is declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// A member of an entity kind (`entities: <kind>: { members | add }`).
    EntityMember,
    /// A member of an enum (`enums:`, or a state row's `{ enum: [...] }`).
    EnumMember,
    /// A def (`defs:`).
    Def,
    /// A season (`seasons:`).
    Season,
    /// A relation (`relations:`).
    Relation,
    /// A speaker of a `cast:` (a schema's or a plugin's).
    Cast,
    /// A scene, beat, entry or choice id: a name a play or test script's
    /// `pick:`/`winner:` holds, and a choice is also a recorded value.
    Id,
    /// A segment of a state path — a `state:` row's segments, and the ids
    /// that become one: quest, objective, entry, branch and hub ids.
    PathSegment,
    /// An occasion a plugin declares.
    Occasion,
    /// An event a plugin declares.
    Event,
    /// A directive a plugin declares.
    Directive,
    /// A beat template's param.
    BeatTemplateParam,
    /// A component's param.
    ComponentParam,
}

impl Slot {
    /// The declarations this slot covers, as the reference table lists them.
    pub fn label(self) -> &'static str {
        match self {
            Slot::EntityMember => "entity member",
            Slot::EnumMember => "enum member",
            Slot::Def => "def",
            Slot::Season => "season",
            Slot::Relation => "relation",
            Slot::Cast => "cast id",
            Slot::Id => "scene, beat, entry or choice id",
            Slot::PathSegment => {
                "state path segment (and quest, objective, entry, branch or hub id)"
            }
            Slot::Occasion => "plugin occasion",
            Slot::Event => "plugin event",
            Slot::Directive => "plugin directive",
            Slot::BeatTemplateParam => "beat template param",
            Slot::ComponentParam => "component param",
        }
    }

    /// The code a refusal in this slot is reported under: names a plugin
    /// exports are its package's (`E-PLUGIN-RESERVED-NAME`), template params
    /// are `E-TEMPLATE`'s, everything a project declares `E-RESERVED-NAME`.
    pub fn code(self) -> &'static str {
        match self {
            Slot::Occasion | Slot::Event | Slot::Directive => "E-PLUGIN-RESERVED-NAME",
            Slot::BeatTemplateParam | Slot::ComponentParam => "E-TEMPLATE",
            _ => "E-RESERVED-NAME",
        }
    }
}

/// One row of the table: names that already mean something, the slots they
/// are refused in, and what they mean.
#[derive(Debug)]
pub struct Group {
    pub names: &'static [&'static str],
    pub slots: &'static [Slot],
    /// What each name already is, for the table; completes "`<name>` is …".
    pub is: &'static str,
    /// What one refused name already is, for its diagnostic: the same
    /// clause with the example written for that name and slot.
    pub says: fn(&str, Slot) -> String,
}

/// The state roots: a bare one in a condition starts a state path.
pub const STATE_ROOTS: &[&str] = &[
    "scene", "run", "user", "app", "quest", "entry", "prev", "clock", "occasion", "season",
];

/// CEL's literal words.
pub const CEL_LITERALS: &[&str] = &["true", "false", "null"];

/// CEL's reserved words (besides its literals): none can be written as an
/// identifier, so none can be a path segment or a relation.
pub const CEL_KEYWORDS: &[&str] = &[
    "as",
    "break",
    "const",
    "continue",
    "else",
    "for",
    "function",
    "if",
    "import",
    "in",
    "let",
    "loop",
    "namespace",
    "package",
    "return",
    "var",
    "void",
    "while",
];

/// The Lute-CEL profile's calls and CEL's macros: inside a fact query each
/// parses as the call, never as a relation.
pub const CEL_CALLS: &[&str] = &[
    "all",
    "count",
    "countDistinct",
    "exists",
    "exists_one",
    "filter",
    "has",
    "holds",
    "isSet",
    "map",
    "now",
    "validAt",
    "visited",
];

/// The core statements (`::set{…}`, `::use{component=…}`): a plugin cannot
/// declare a directive of the same name.
pub const CORE_STATEMENT_NAMES: &[&str] =
    &["set", "assert", "retract", "accept", "use", "body", "cut"];

/// The core block tags (`<match>`, `<quest>`): a plugin cannot declare a
/// directive of the same name.
pub const CORE_TAG_NAMES: &[&str] = &[
    "scene",
    "on",
    "quest",
    "objective",
    "match",
    "branch",
    "hub",
    "choice",
    "when",
    "otherwise",
    "entry",
    "beat",
    "timeline",
    "track",
    "reward",
    "return",
];

/// The keys that make a play-script step do something (`- newRun: true`).
/// The play parser reads its action keys from here.
pub const PLAY_STEP_ACTIONS: &[&str] = &["occasion", "newRun", "engine", "event", "advance", "end"];

/// A beat template's header keys: a `<beat use=…>` attribute of the same
/// name sets the beat's own key, never the param.
pub const BEAT_TEMPLATE_PARAM_NAMES: &[&str] = &[
    "id", "use", "on", "target", "for", "title", "priority", "once", "share", "after", "when",
    "spentBy", "also",
];

/// A `::use` directive's own keys: a component param of the same name can
/// never be passed.
pub const COMPONENT_PARAM_NAMES: &[&str] = &["component", "when"];

/// The `once`/tier periods a season name would sit beside.
const PERIODS: &[&str] = &["run", "user", "day", "week", "slot"];

/// The table. [`refusal`] takes the first row naming both the name and the
/// slot, so a more specific row comes first.
pub const GROUPS: &[Group] = &[
    Group {
        names: &["unset"],
        slots: &[Slot::EntityMember, Slot::EnumMember, Slot::Id],
        is: "the no-value word: `is=\"unset\"` and `== 'unset'` test for a path that holds \
             nothing, so a value named `unset` can never be matched",
        says: |_, _| {
            "the no-value word: `is=\"unset\"` and `== 'unset'` test for a path that holds \
             nothing, never for a value named `unset`"
                .into()
        },
    },
    Group {
        names: CEL_LITERALS,
        slots: &[
            Slot::EntityMember,
            Slot::EnumMember,
            Slot::Id,
            Slot::Relation,
            Slot::Season,
            Slot::PathSegment,
        ],
        is: "a CEL literal: in a condition and in `is=` it is read as the value, never as a name",
        says: |name, _| {
            format!("a CEL literal: a condition and `is=\"{name}\"` read `{name}` as the value, never as a name")
        },
    },
    Group {
        names: &["_"],
        slots: &[Slot::EntityMember, Slot::EnumMember],
        is: "the wildcard of fact patterns (`holds(knows(_))`) and the fallback key of `per:` \
             defaults",
        says: |_, _| {
            "the wildcard of fact patterns (`holds(knows(_))`) and the fallback key of `per:` \
             defaults"
                .into()
        },
    },
    Group {
        names: &["none"],
        slots: &[Slot::Id],
        is: "the play and test word for no pick and no winner (`pick: none`, `winner: none`)",
        says: |_, _| {
            "the play and test word for no pick and no winner (`pick: none`, `winner: none`)".into()
        },
    },
    Group {
        names: STATE_ROOTS,
        slots: &[Slot::EntityMember, Slot::Def],
        is: "a state root: in a condition a bare root name starts a state path, so \
             `holds(found(clock))` and `@clock` read state instead",
        says: |name, slot| match slot {
            Slot::Def => format!(
                "a state root: `@{name}` reads as a bare `{name}`, which starts a state path, \
                 never the def"
            ),
            _ => format!(
                "a state root: in a condition a bare `{name}` starts a state path, so a fact \
                 query naming this member (`holds(<relation>({name}))`) reads `{name}` state \
                 instead"
            ),
        },
    },
    Group {
        names: &[
            "scene", "run", "user", "app", "quest", "entry", "prev", "clock", "occasion", "season",
            "day", "week", "slot",
        ],
        slots: &[Slot::Season],
        is: "a state root or a `once`/tier period, so `once=\"season:run\"` would sit beside \
             `once=\"run\"` meaning something else",
        says: |name, _| {
            if PERIODS.contains(&name) {
                format!(
                    "a `once`/tier period: `once=\"season:{name}\"` would sit beside \
                     `once=\"{name}\"` meaning something else"
                )
            } else {
                format!(
                    "a state root: `season.{name}.…` and `once=\"season:{name}\"` would name the \
                     season with the word that starts `{name}.…` state paths"
                )
            }
        },
    },
    Group {
        names: CEL_KEYWORDS,
        slots: &[
            Slot::EntityMember,
            Slot::Relation,
            Slot::Season,
            Slot::PathSegment,
        ],
        is: "a CEL keyword, which a condition cannot write as a name (`quest.in.state` does not \
             parse)",
        says: |name, slot| {
            let example = match slot {
                Slot::EntityMember => format!("a fact query naming `{name}` does not parse"),
                Slot::Relation => format!("`holds({name}(…))` does not parse"),
                Slot::Season => format!("`season.{name}.…` does not parse"),
                _ => format!("a state path through `{name}` does not parse"),
            };
            format!("a CEL keyword, which a condition cannot write as a name ({example})")
        },
    },
    Group {
        names: CEL_CALLS,
        slots: &[Slot::Relation],
        is: "a Lute-CEL call or CEL macro, so `holds(<name>(…))` parses as the call",
        says: |name, _| {
            format!("a Lute-CEL call or CEL macro: `holds({name}(…))` parses as the call")
        },
    },
    Group {
        names: &["completed", "active"],
        slots: &[Slot::Relation],
        is: "an `after:` call (`completed(\"<quest>\")`, `active(\"<quest>\")`), so \
             `after=\"completed(dorm)\"` would read the quest call",
        says: |name, _| {
            format!(
                "an `after:` call: `after=\"{name}(…)\"` reads the quest call \
                 `{name}(\"<quest>\")`, never this relation"
            )
        },
    },
    Group {
        names: &["cel", "not"],
        slots: &[Slot::Relation],
        is: "a rule word: in `rules:` `not(…)` negates and `cel(\"…\")` is a condition",
        says: |name, _| match name {
            "not" => "a rule word: in `rules:` `not(…)` negates, never matches a `not` fact".into(),
            _ => "a rule word: in `rules:` `cel(\"…\")` is a condition, never a `cel` fact".into(),
        },
    },
    Group {
        names: &["narrator"],
        slots: &[Slot::Cast],
        is: "the built-in narration speaker: `@narrator:` lines are narration, so a cast entry \
             for it is never shown and its `present:` guards every narrated line",
        says: |_, _| {
            "the built-in narration speaker: `@narrator:` lines are narration, a cast entry for \
             it is never shown, and its `present:` would guard every narrated line"
                .into()
        },
    },
    Group {
        names: BUILTIN_LIFECYCLE_EVENTS,
        slots: &[Slot::Occasion, Slot::Event],
        is: "an engine lifecycle event (`<on event=\"questComplete\">`)",
        says: |name, _| format!("an engine lifecycle event (`<on event=\"{name}\">`)"),
    },
    Group {
        names: PLAY_STEP_ACTIONS,
        slots: &[Slot::Occasion],
        is: "a play-script step key: `- newRun: true` starts a new run and `- end: true` ends \
             the play, neither raises an occasion of that name",
        says: |name, _| {
            format!(
                "a play-script step key: `- {name}: …` is a step of its own, never raises an \
                 occasion `{name}`"
            )
        },
    },
    Group {
        names: CORE_STATEMENT_NAMES,
        slots: &[Slot::Directive],
        is: "a core statement (`::set{…}`, `::use{component=…}`), which content always reads \
             as the core one",
        says: |name, _| {
            format!("a core statement: content always reads `::{name}{{…}}` as the core one")
        },
    },
    Group {
        names: CORE_TAG_NAMES,
        slots: &[Slot::Directive],
        is: "a core block tag (`<match>`, `<quest>`), which content always reads as the core one",
        says: |name, _| {
            format!("a core block tag (`<{name}>`), which content always reads as the core one")
        },
    },
    Group {
        names: BEAT_TEMPLATE_PARAM_NAMES,
        slots: &[Slot::BeatTemplateParam],
        is: "a beat header key: `<beat use=… when=…>` sets the beat's own `when`, never the \
             param",
        says: |name, _| {
            format!(
                "a beat header key: `<beat use=… {name}=…>` sets the beat's own `{name}`, never \
                 the param"
            )
        },
    },
    Group {
        names: COMPONENT_PARAM_NAMES,
        slots: &[Slot::ComponentParam],
        is: "a `::use` key of its own (`::use{component=… when=…}`), so the param could never \
             be passed",
        says: |name, _| {
            format!(
                "a `::use` key of its own: `::use{{component=… {name}=…}}` sets the `::use`'s \
                 own `{name}`, never the param"
            )
        },
    },
];

/// A declared name the table refuses in its slot.
#[derive(Clone, Copy, Debug)]
pub struct Refusal {
    pub name: &'static str,
    pub slot: Slot,
    pub group: &'static Group,
}

impl Refusal {
    /// The code this refusal is reported under ([`Slot::code`]).
    pub fn code(&self) -> &'static str {
        self.slot.code()
    }

    /// A name to use instead ([`instead`]).
    pub fn instead(&self) -> String {
        instead(self.name, self.slot)
    }

    /// The whole sentence: that `name` cannot be `what` (the declaration,
    /// e.g. "a def" or "a member of entity kind `crew`"), what it already
    /// is, and a name to use instead.
    pub fn message(&self, what: &str) -> String {
        format!(
            "`{}` cannot name {what} because it is {} — rename it (e.g. `{}`)",
            self.name,
            (self.group.says)(self.name, self.slot),
            self.instead()
        )
    }
}

/// The row refusing `name` in `slot`, if any (case-sensitive, like every
/// Lute name).
pub fn refusal(slot: Slot, name: &str) -> Option<Refusal> {
    GROUPS.iter().find_map(|group| {
        let name = *group.names.iter().find(|n| **n == name)?;
        group
            .slots
            .contains(&slot)
            .then_some(Refusal { name, slot, group })
    })
}

/// `true` for a name CEL itself cannot take as a relation in a fact query
/// (its literals, keywords, and the calls/macros that parse first).
pub fn is_cel_word(name: &str) -> bool {
    CEL_LITERALS.contains(&name) || CEL_KEYWORDS.contains(&name) || CEL_CALLS.contains(&name)
}

/// A replacement for a refused `name` in `slot`: a fixed word where one
/// reads naturally, otherwise the name with a prefix or suffix fitting the
/// slot (`onNewRun` for an occasion, `runSeason`, `theClock`).
pub fn instead(name: &str, slot: Slot) -> String {
    let fixed = match name {
        "unset" => Some("notSet"),
        "true" => Some("yes"),
        "false" => Some("no"),
        "null" => Some("empty"),
        "none" => Some("nothing"),
        "_" => Some("other"),
        "narrator" => Some("voice"),
        "visited" => Some("wasAt"),
        "count" => Some("tally"),
        "completed" => Some("finished"),
        "active" => Some("ongoing"),
        "in" => Some("inside"),
        _ => None,
    };
    let generic = matches!(
        slot,
        Slot::Occasion | Slot::Event | Slot::Season | Slot::Def | Slot::Directive
    );
    if let (Some(word), false) = (fixed, generic) {
        return word.to_string();
    }
    let cap = {
        let mut c = name.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default()
    };
    match slot {
        Slot::Occasion | Slot::Event => format!("on{cap}"),
        Slot::Season => format!("{name}Season"),
        Slot::Def => format!("is{cap}"),
        Slot::Relation => format!("is{cap}"),
        Slot::Directive | Slot::BeatTemplateParam | Slot::ComponentParam => format!("my{cap}"),
        _ => format!("the{cap}"),
    }
}

/// The slots of `group`, as the table lists them.
fn slot_labels(group: &Group) -> String {
    group
        .slots
        .iter()
        .map(|s| s.label())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The codes `group`'s refusals are reported under.
fn group_codes(group: &Group) -> String {
    let mut codes: Vec<&str> = group.slots.iter().map(|s| s.code()).collect();
    codes.dedup();
    if group.slots.contains(&Slot::Cast) {
        codes.push("E-PLUGIN-RESERVED-NAME");
    }
    codes.join(", ")
}

fn backticked(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The table as plain text, for `lute --explain`.
pub fn render_text() -> String {
    let mut out = String::from(
        "Reserved names — each is refused where it is declared, with a name to use instead:\n",
    );
    for group in GROUPS {
        out.push_str(&format!(
            "\n  {}\n    refused as: {}\n    because it is {}\n    reported as: {}\n",
            group.names.join(" "),
            slot_labels(group),
            group.is,
            group_codes(group)
        ));
    }
    out
}

/// The table as the Markdown rows of the reference page's "Reserved names"
/// section (the header row included).
pub fn render_markdown() -> String {
    let mut out =
        String::from("| Names | Refused as | Because it is | Code |\n| --- | --- | --- | --- |\n");
    for group in GROUPS {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            backticked(group.names),
            slot_labels(group),
            group.is.replace('|', "\\|"),
            group_codes(group)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_matches_name_and_slot() {
        let r = refusal(Slot::Def, "run").expect("state root refused as a def");
        assert_eq!(r.code(), "E-RESERVED-NAME");
        assert!(
            refusal(Slot::EnumMember, "run").is_none(),
            "an enum member may be `run`"
        );
        assert!(
            refusal(Slot::EnumMember, "none").is_none(),
            "an enum member may be `none`"
        );
        assert!(refusal(Slot::Id, "none").is_some());
        assert_eq!(
            refusal(Slot::Occasion, "questComplete").map(|r| r.code()),
            Some("E-PLUGIN-RESERVED-NAME")
        );
    }

    #[test]
    fn every_message_names_a_replacement_that_is_not_reserved() {
        for group in GROUPS {
            for slot in group.slots {
                for name in group.names {
                    let r = refusal(*slot, name).expect("row refuses its own names");
                    let instead = r.instead();
                    assert_ne!(&instead, name);
                    assert!(
                        refusal(*slot, &instead).is_none(),
                        "`{instead}` (for `{name}` as {slot:?}) is itself reserved"
                    );
                }
            }
        }
    }
}

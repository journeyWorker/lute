use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_compile::ir::Command;
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_manifest::bindings::{Bindings, Idle};
use lute_manifest::schema::OccasionTarget;
use lute_model::ProjectModel;

pub(crate) fn check_model(model: &ProjectModel) -> Vec<(PathBuf, Diagnostic)> {
    let path = model.root().join("player.bindings.yaml");
    if !path.is_file() {
        return Vec::new();
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return vec![(
                path,
                diag(
                    "E-BINDING-SHAPE",
                    format!("cannot read player bindings: {error}"),
                    Span {
                        byte_start: 0,
                        byte_end: 0,
                        line: 0,
                        column: 0,
                        utf16_range: (0, 0),
                    },
                ),
            )]
        }
    };
    if let Some(message) = effect_shape_error(&text) {
        return vec![(
            path,
            diag("E-BINDING-EFFECT", message, locate(&text, "effect")),
        )];
    }
    let bindings = match serde_yaml::from_str::<Bindings>(&text) {
        Ok(value) => value,
        Err(error) => {
            let span = error.location().map_or_else(
                || locate(&text, "bindingsVersion"),
                |loc| {
                    let at = line_offset(&text, loc.line(), loc.column());
                    Span::from_bytes(
                        &TextIndex::new(&text),
                        at,
                        at.saturating_add(1).min(text.len()),
                    )
                },
            );
            return vec![(path, diag("E-BINDING-SHAPE", error.to_string(), span))];
        }
    };
    let mut out = Vec::new();
    let anchor = |needle: &str| locate(&text, needle);
    let mut domains = BTreeMap::<String, BTreeSet<String>>::new();
    let mut cast = BTreeSet::new();
    let mut occasions = BTreeMap::new();
    for document in model.documents() {
        for (name, domain) in &document.folded.domains {
            domains
                .entry(name.clone())
                .or_default()
                .extend(domain.members.iter().cloned());
        }
        cast.extend(document.folded.cast.keys().cloned());
        occasions.extend(document.folded.occasions.clone());
    }
    check_domain_map(
        &bindings.anchors,
        "anchor",
        domains.get("anchor"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.actions,
        "action",
        domains.get("action"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.transitions,
        "transition",
        domains.get("transition"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.framing,
        "framing",
        domains.get("framing"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.camera_move,
        "cameraMove",
        domains.get("cameraMove"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.cg_layouts,
        "cgLayout",
        domains.get("cgLayout"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.text_styles,
        "textStyle",
        domains.get("textStyle"),
        &anchor,
        &mut out,
    );
    check_domain_map(
        &bindings.sequences,
        "sequence",
        domains.get("sequence"),
        &anchor,
        &mut out,
    );
    // 0.42.0 §3.2: one gain per `volume` member, each finite in [0, 1].
    check_domain_map(
        &bindings.volumes,
        "volume",
        domains.get("volume"),
        &anchor,
        &mut out,
    );
    for (member, gain) in &bindings.volumes {
        if !(gain.is_finite() && (0.0..=1.0).contains(gain)) {
            use lute_manifest::yaml_text::{yaml_span, YamlStep};
            let span = yaml_span(
                &text,
                &[YamlStep::Key("volumes"), YamlStep::Key(member), YamlStep::Value],
            )
            .unwrap_or_else(|| anchor(member));
            out.push((
                path.clone(),
                diag(
                    "E-BINDING-SHAPE",
                    format!(
                        "volume binding `{member}` gain must be a finite number in [0, 1], \
                         got `{gain}`"
                    ),
                    span,
                ),
            ));
        }
    }
    for (id, character) in &bindings.characters {
        if !cast.contains(id) {
            out.push((
                path.clone(),
                diag(
                    "E-BINDING-KEY",
                    unknown_key("character", id, &cast),
                    anchor(id),
                ),
            ));
        }
        check_domain_keys(
            &character.sprite.emotions,
            "emotion",
            domains.get("emotion"),
            &anchor,
            &mut out,
        );
        for (costume, sprite) in &character.costumes {
            if let Some(members) = domains.get("costume") {
                if !members.contains(costume) {
                    out.push((
                        path.clone(),
                        diag(
                            "E-BINDING-KEY",
                            unknown_key("costume", costume, members),
                            anchor(costume),
                        ),
                    ));
                }
            }
            check_domain_keys(
                &sprite.emotions,
                "emotion",
                domains.get("emotion"),
                &anchor,
                &mut out,
            );
        }
    }
    let mut used = Usage::default();
    for document in model.documents() {
        if let Some(artifact) = &document.artifact {
            for command in &artifact.commands {
                used.add(command);
            }
        }
    }
    usage_check(&path, &text, &bindings, &used, &anchor, &mut out);
    files_check(model.root(), &bindings, &path, &text, &anchor, &mut out);
    for character in used.characters {
        if !bindings.characters.contains_key(&character) {
            out.push((
                path.clone(),
                diag(
                    "W-BINDING-MISSING",
                    format!(
                        "cast member `{character}` appears on stage but has no character binding"
                    ),
                    anchor(&character),
                ),
            ));
        }
    }
    check_flow(&bindings, &occasions, &domains, &path, &anchor, &mut out);
    for (diagnostic_path, _) in &mut out {
        if diagnostic_path.as_os_str().is_empty() {
            *diagnostic_path = path.clone();
        }
    }
    out
}

#[derive(Default)]
struct Usage {
    assets: BTreeSet<String>,
    backgrounds: BTreeSet<String>,
    times: BTreeSet<String>,
    moods: BTreeSet<String>,
    sfx: BTreeSet<String>,
    vfx: BTreeSet<String>,
    characters: BTreeSet<String>,
}
impl Usage {
    fn add(&mut self, command: &Command) {
        match command {
            Command::Bg(c) => {
                if let Some(v) = &c.location {
                    self.backgrounds.insert(v.clone());
                }
                if let Some(v) = &c.time {
                    self.times.insert(v.clone());
                }
                if let Some(v) = &c.asset_id {
                    self.assets.insert(v.clone());
                }
            }
            Command::Music(c) => {
                if let Some(v) = &c.mood {
                    self.moods.insert(v.clone());
                }
                if let Some(v) = &c.asset_id {
                    self.assets.insert(v.clone());
                }
            }
            Command::Sfx(c) => {
                if let Some(v) = &c.sound {
                    self.sfx.insert(v.clone());
                }
                if let Some(v) = &c.asset_id {
                    self.assets.insert(v.clone());
                }
            }
            Command::Vfx(c) => {
                self.vfx.insert(c.r#type.clone());
            }
            Command::Actor(c) => {
                self.characters.insert(c.character.clone());
            }
            Command::Cg(c) => {
                self.assets.insert(c.asset_id.clone());
            }
            Command::Video(c) => {
                self.assets.insert(c.asset_id.clone());
            }
            Command::Sequence(_)
            | Command::Camera(_)
            | Command::Line(_)
            | Command::Set(_)
            | Command::Assert(_)
            | Command::Retract(_)
            | Command::Choice(_)
            | Command::Match(_)
            | Command::Hub(_)
            | Command::Jump(_)
            | Command::End(_)
            | Command::Barrier(_)
            | Command::Quest(_)
            | Command::On(_)
            | Command::Plugin(_)
            | Command::Entry(_)
            | Command::Accept(_)
            | Command::Beat(_) => {}
        }
    }
}

fn usage_check(
    path: &Path,
    text: &str,
    bindings: &Bindings,
    used: &Usage,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    for key in &used.assets {
        if !bindings.assets.contains_key(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("asset `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in bindings.assets.keys() {
        if !used.assets.contains(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-UNUSED",
                    format!("asset binding `{key}` is unused"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in &used.backgrounds {
        if !bindings.backgrounds.contains_key(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("background `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in bindings.backgrounds.keys() {
        if !used.backgrounds.contains(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-UNUSED",
                    format!("background binding `{key}` is unused"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in &used.times {
        if !bindings
            .backgrounds
            .values()
            .any(|bg| bg.times.contains_key(key))
        {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("background time `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in &used.moods {
        if !bindings.music.moods.contains_key(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("music mood `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in bindings.music.moods.keys() {
        if !used.moods.contains(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-UNUSED",
                    format!("music mood binding `{key}` is unused"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in &used.sfx {
        if !bindings.sfx.contains_key(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("sfx `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in bindings.sfx.keys() {
        if !used.sfx.contains(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-UNUSED",
                    format!("sfx binding `{key}` is unused"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in &used.vfx {
        if !bindings.vfx.contains_key(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-MISSING",
                    format!("vfx `{key}` is used but has no binding"),
                    anchor(key),
                ),
            ));
        }
    }
    for key in bindings.vfx.keys() {
        if !used.vfx.contains(key) {
            out.push((
                path.to_path_buf(),
                diag(
                    "W-BINDING-UNUSED",
                    format!("vfx binding `{key}` is unused"),
                    anchor(key),
                ),
            ));
        }
    }
    let _ = text;
}

fn files_check(
    root: &Path,
    b: &Bindings,
    path: &Path,
    text: &str,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let base = path.parent().unwrap_or(root).join(&b.assets_base);
    let mut files = Vec::new();
    files.extend(b.assets.values());
    files.extend(b.backgrounds.values().map(|v| &v.image));
    for v in b.backgrounds.values() {
        files.extend(v.times.values());
    }
    for c in b.characters.values() {
        files.push(&c.sprite.default);
        files.extend(c.sprite.emotions.values());
        for s in c.costumes.values() {
            files.push(&s.default);
            files.extend(s.emotions.values());
        }
    }
    files.extend(b.music.moods.values());
    files.extend(b.sfx.values());
    files.extend(b.sequences.values().map(|v| &v.video));
    if let Some(title) = &b.ui.title {
        files.push(&title.background);
    }
    for file in files {
        if !base.join(file).is_file() {
            out.push((
                path.to_path_buf(),
                diag(
                    "E-BINDING-FILE",
                    format!(
                        "binding file `{file}` does not exist relative to assetsBase `{}`",
                        b.assets_base
                    ),
                    anchor(file),
                ),
            ));
        }
    }
    let _ = text;
}

fn check_flow(
    b: &Bindings,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
    domains: &BTreeMap<String, BTreeSet<String>>,
    path: &Path,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    if let Some(flow) = &b.flow.new_game {
        flow_ref(
            &flow.occasion,
            flow.target.as_deref(),
            occasions,
            domains,
            path,
            anchor,
            out,
        );
    }
    if let Some(Idle::Menu { menu }) = &b.flow.idle {
        for item in menu {
            flow_ref(
                &item.occasion,
                item.target.as_deref(),
                occasions,
                domains,
                path,
                anchor,
                out,
            );
        }
    }
    if let Some(Idle::Raise { raise }) = &b.flow.idle {
        flow_ref(
            &raise.occasion,
            raise.target.as_deref(),
            occasions,
            domains,
            path,
            anchor,
            out,
        );
    }
}
fn flow_ref(
    occasion: &str,
    target: Option<&str>,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
    domains: &BTreeMap<String, BTreeSet<String>>,
    path: &Path,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let Some(decl) = occasions.get(occasion) else {
        out.push((
            path.to_path_buf(),
            diag(
                "E-BINDING-KEY",
                format!(
                    "flow occasion `{occasion}` is not declared{}",
                    suggest(occasion, occasions.keys())
                ),
                anchor(occasion),
            ),
        ));
        return;
    };
    let Some(target) = target else {
        return;
    };
    if let OccasionTarget::Domain {
        entity, members, ..
    } = &decl.target
    {
        let allowed = members
            .as_ref()
            .map(|m| m.iter().cloned().collect())
            .or_else(|| domains.get(entity).cloned());
        if let Some(allowed) = allowed {
            if !allowed.contains(target) {
                out.push((
                    path.to_path_buf(),
                    diag(
                        "E-BINDING-KEY",
                        format!(
                            "flow target `{target}` is not a member of occasion `{occasion}`{}",
                            suggest(target, allowed.iter())
                        ),
                        anchor(target),
                    ),
                ));
            }
        }
    }
}

fn check_domain_map<T>(
    map: &BTreeMap<String, T>,
    domain: &str,
    members: Option<&BTreeSet<String>>,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    check_domain_keys(map, domain, members, anchor, out);
    if let Some(members) = members {
        for member in members {
            if !map.contains_key(member) {
                out.push((
                    PathBuf::new(),
                    diag(
                        "W-BINDING-MISSING",
                        format!("declared {domain} member `{member}` has no binding"),
                        anchor(member),
                    ),
                ));
            }
        }
    }
}
fn check_domain_keys<T>(
    map: &BTreeMap<String, T>,
    domain: &str,
    members: Option<&BTreeSet<String>>,
    anchor: &impl Fn(&str) -> Span,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    if let Some(members) = members {
        for key in map.keys() {
            if !members.contains(key) {
                out.push((
                    PathBuf::new(),
                    diag(
                        "E-BINDING-KEY",
                        unknown_key(domain, key, members),
                        anchor(key),
                    ),
                ));
            }
        }
    }
}
fn unknown_key(kind: &str, key: &str, members: &BTreeSet<String>) -> String {
    format!(
        "{kind} binding key `{key}` is not declared{}",
        suggest(key, members.iter())
    )
}
fn suggest<'a>(key: &str, values: impl Iterator<Item = &'a String>) -> String {
    let values: Vec<&str> = values.map(String::as_str).collect();
    lute_manifest::suggest::nearest(key, values, 2)
        .map_or_else(String::new, |near| format!("; did you mean `{near}`?"))
}

fn effect_shape_error(text: &str) -> Option<String> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(text).ok()?;
    for section in ["actions", "transitions", "cameraMove", "vfx"] {
        let Some(map) = value.get(section).and_then(serde_yaml::Value::as_mapping) else {
            continue;
        };
        for (name, item) in map {
            let Some(item) = item.as_mapping() else {
                continue;
            };
            let effect = item.get("effect").and_then(serde_yaml::Value::as_str)?;
            let custom = effect == "custom";
            if custom && section != "vfx" {
                return Some(format!(
                    "player effect `custom` is admitted only under `vfx`, not on {section} binding `{name:?}`"
                ));
            }
            if !custom && !known_effect(effect) {
                return Some(format!(
                    "unknown player effect `{effect}` on binding `{name:?}`"
                ));
            }
            let allowed: &[&str] = if custom {
                &["name", "duration"]
            } else {
                effect_params(effect)
            };
            for key in item.keys().filter_map(serde_yaml::Value::as_str) {
                if key != "effect" && !allowed.contains(&key) {
                    return Some(format!(
                        "effect `{effect}` does not accept parameter `{key}` on binding `{name:?}`"
                    ));
                }
            }
        }
    }
    None
}
fn known_effect(effect: &str) -> bool {
    matches!(
        effect,
        "fade"
            | "dissolve"
            | "wipeLeft"
            | "wipeRight"
            | "wipeUp"
            | "wipeDown"
            | "slideLeft"
            | "slideRight"
            | "cut"
            | "fadeIn"
            | "fadeOut"
            | "slideIn"
            | "slideOut"
            | "hop"
            | "shake"
            | "none"
            | "zoom"
            | "pan"
            | "flash"
            | "blur"
            | "tint"
    )
}
fn effect_params(effect: &str) -> &'static [&'static str] {
    match effect {
        "fade" | "dissolve" | "wipeLeft" | "wipeRight" | "wipeUp" | "wipeDown" | "slideLeft"
        | "slideRight" | "fadeIn" | "fadeOut" | "slideIn" | "slideOut" | "hop" => &["duration"],
        "shake" => &["duration", "strength"],
        "zoom" => &["duration", "scale"],
        "pan" => &["duration", "x", "y"],
        "flash" => &["duration", "color"],
        "blur" => &["duration", "strength"],
        "tint" => &["duration", "color"],
        "cut" | "none" => &[],
        _ => &["duration"],
    }
}
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: if code.starts_with('W') {
            Severity::Warning
        } else {
            Severity::Error
        },
        message,
        span,
        layer: Layer::Staging,
        evidence: None,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
fn locate(text: &str, needle: &str) -> Span {
    let at = text.find(needle).unwrap_or(0);
    Span::from_bytes(
        &TextIndex::new(text),
        at,
        at.saturating_add(needle.len()).min(text.len()),
    )
}
fn line_offset(text: &str, line: usize, column: usize) -> usize {
    text.split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>()
        + column.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn effect_validation_distinguishes_unknown_effect_and_parameter() {
        assert!(super::effect_shape_error(
            "actions:\n  hop: {effect: hop, strength: 2}\n"
        ).is_some());
        assert!(super::effect_shape_error(
            "actions:\n  hop: {effect: teleport}\n"
        ).is_some());
    }

    #[test]
    fn custom_effect_is_admitted_only_under_vfx() {
        assert_eq!(
            super::effect_shape_error(
                "vfx:\n  lanterns: {effect: custom}\n  tideRise: {effect: custom, name: tide-rise, duration: 2}\n"
            ),
            None
        );
        for section in ["actions", "transitions", "cameraMove"] {
            let message = super::effect_shape_error(&format!("{section}:\n  x: {{effect: custom}}\n"))
                .expect("custom outside vfx is rejected");
            assert!(message.contains("only under `vfx`"), "{message}");
        }
        let message = super::effect_shape_error("vfx:\n  x: {effect: custom, color: red}\n")
            .expect("custom keeps a closed parameter set");
        assert!(message.contains("does not accept parameter `color`"), "{message}");
    }
}

use std::fmt;
use std::path::{Component, Path, PathBuf};

use lute_semantic::{NodeKey, NodeKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    AbsolutePath(PathBuf),
    EscapesRoot(PathBuf),
    EmptyPath,
    MissingNodeSeparator,
    UnknownNodeKind(String),
    EmptyNodeKey,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbsolutePath(path) => {
                write!(formatter, "path `{}` must be relative", path.display())
            }
            Self::EscapesRoot(path) => {
                write!(formatter, "path `{}` escapes project root", path.display())
            }
            Self::EmptyPath => formatter.write_str("path must not be empty"),
            Self::MissingNodeSeparator => formatter.write_str("node key must be `kind:key`"),
            Self::UnknownNodeKind(kind) => write!(formatter, "unknown node kind `{kind}`"),
            Self::EmptyNodeKey => formatter.write_str("node key is empty"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub(crate) fn safe_relative(path: &Path) -> Result<PathBuf, DecodeError> {
    if path.is_absolute() {
        return Err(DecodeError::AbsolutePath(path.to_path_buf()));
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DecodeError::EscapesRoot(path.to_path_buf()));
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(DecodeError::EmptyPath);
    }
    Ok(PathBuf::from(
        out.to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
    ))
}

pub(crate) fn parse_node_key(text: &str) -> Result<NodeKey, DecodeError> {
    let (kind, key) = text
        .split_once(':')
        .ok_or(DecodeError::MissingNodeSeparator)?;
    let kind = match kind {
        "project" => NodeKind::Project,
        "document" => NodeKind::Document,
        "scene" => NodeKind::Scene,
        "beat" => NodeKind::Beat,
        "shot" => NodeKind::Shot,
        "line" => NodeKind::Line,
        "choice" => NodeKind::Choice,
        "quest" => NodeKind::Quest,
        "objective" => NodeKind::Objective,
        "reward" => NodeKind::Reward,
        "entry" => NodeKind::Entry,
        "occasion" => NodeKind::Occasion,
        "relation" => NodeKind::Relation,
        "state" => NodeKind::State,
        "def" => NodeKind::Def,
        "component" => NodeKind::Component,
        "expanded" => NodeKind::Expanded,
        "fact" => NodeKind::Fact,
        "clock" => NodeKind::Clock,
        "engine" => NodeKind::Engine,
        _ => return Err(DecodeError::UnknownNodeKind(kind.to_owned())),
    };
    if key.is_empty() {
        return Err(DecodeError::EmptyNodeKey);
    }
    Ok(NodeKey::new(kind, key))
}

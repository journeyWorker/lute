use super::Preserve;

pub(crate) fn name(item: &Preserve) -> &'static str {
    match item {
        Preserve::Ids(_) => "ids",
        Preserve::LineIds => "lineIds",
        Preserve::VoiceKeys => "voiceKeys",
        Preserve::ChoiceEffects(_) => "choiceEffects",
        Preserve::Rewards(_) => "rewards",
        Preserve::HostContracts => "hostContracts",
        Preserve::Conditions(_) => "conditions",
        Preserve::Reachability(_) => "reachability",
        Preserve::Constraints => "constraints",
    }
}

use std::process::Command;
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples/games/drowned-crown");

struct E { g: &'static str, k: &'static str, e: &'static str, c: &'static [(&'static str, &'static str, u32)] }

const FELLED: &[E] = &[
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13
    E { g: "beats", k: "occasion:lastDive", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19
    E { g: "beats", k: "scene:last.crown", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19
    E { g: "beats", k: "scene:last.rail", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19
    E { g: "beats", k: "scene:last.throne", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9
    E { g: "disclosures", k: "beat:bonds.brann1", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:14
    E { g: "disclosures", k: "beat:bonds.brann2", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 14)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:38; lore/bonds.lute:26
    E { g: "disclosures", k: "beat:bonds.quill1", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 38), ("reads", "lore/bonds.lute", 26)] },
    // source: lore/brann.lute:26
    E { g: "disclosures", k: "beat:brann.regentTold", e: "proven", c: &[("queries", "lore/brann.lute", 26)] },
    // source: lore/palace.lute:20
    E { g: "disclosures", k: "entry:codexThrone", e: "proven", c: &[("queries", "lore/palace.lute", 20)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28; lore/keepsakes.lute:14
    E { g: "disclosures", k: "entry:keepLantern", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28), ("queries", "lore/keepsakes.lute", 14)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:45; lore/keepsakes.lute:18
    E { g: "disclosures", k: "entry:keepShell", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("asserts", "quests/crew.lute", 45), ("queries", "lore/keepsakes.lute", 18)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:32; lore/quill.lute:38
    E { g: "disclosures", k: "entry:quillAfter", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 32), ("reads", "lore/quill.lute", 38)] },
    // source: lore/quill.lute:26
    E { g: "disclosures", k: "entry:quillHeir", e: "proven", c: &[("queries", "lore/quill.lute", 26)] },
    // source: world.schema.yaml:65; lore/quill.lute:30
    E { g: "disclosures", k: "entry:quillWitness", e: "proven", c: &[("derives", "world.schema.yaml", 65), ("queries", "lore/quill.lute", 30)] },
    // source: world.schema.yaml:77
    E { g: "downstream", k: "def:readyForCrown", e: "proven", c: &[("queries", "world.schema.yaml", 77)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28
    E { g: "downstream", k: "fact:gifted(brann,lantern)", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:45
    E { g: "downstream", k: "fact:gifted(quill,shell)", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("asserts", "quests/crew.lute", 45)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:63
    E { g: "downstream", k: "fact:heard(brann,regent)", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 63)] },
    // source: world.schema.yaml:65
    E { g: "downstream", k: "fact:heard(quill,regent)", e: "proven", c: &[("derives", "world.schema.yaml", 65)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(sefa,regent)", e: "heuristic", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(tavi,regent)", e: "heuristic", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64)] },
    // source: lore/brann.lute:26; lore/brann.lute:31
    E { g: "downstream", k: "fact:told(brann,regent)", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; world.schema.yaml:67
    E { g: "downstream", k: "fact:trusts(brann)", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("derives", "world.schema.yaml", 67)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:19
    E { g: "downstream", k: "state:quest.brannOath.state", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("writes", "quests/crew.lute", 19)] },
    // source: quests/library.lute:10; quests/library.lute:10
    E { g: "downstream", k: "state:quest.libraryKey.state", e: "proven", c: &[("queries", "quests/library.lute", 10), ("writes", "quests/library.lute", 10)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:32
    E { g: "downstream", k: "state:quest.quillMemory.state", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 32)] },
    // source: lore/brann.lute:26; lore/brann.lute:32
    E { g: "downstream", k: "state:user.bond.brann", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:38
    E { g: "downstream", k: "state:user.bond.quill", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 38)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9; lore/bonds.lute:10
    E { g: "lines", k: "line:bonds.brann1.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9), ("contains", "lore/bonds.lute", 10)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9; lore/bonds.lute:11
    E { g: "lines", k: "line:bonds.brann1.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9), ("contains", "lore/bonds.lute", 11)] },
    // source: lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:14; lore/bonds.lute:15
    E { g: "lines", k: "line:bonds.brann2.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 14), ("contains", "lore/bonds.lute", 15)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:38; lore/bonds.lute:26; lore/bonds.lute:27
    E { g: "lines", k: "line:bonds.quill1.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 38), ("reads", "lore/bonds.lute", 26), ("contains", "lore/bonds.lute", 27)] },
    // source: lore/brann.lute:26; lore/brann.lute:28
    E { g: "lines", k: "line:brann.regentTold.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 28)] },
    // source: lore/brann.lute:26; lore/brann.lute:30
    E { g: "lines", k: "line:brann.regentTold.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 30)] },
    // source: lore/brann.lute:26; lore/brann.lute:27
    E { g: "lines", k: "line:brann.regentTold.ilo_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 27)] },
    // source: lore/brann.lute:26; lore/brann.lute:29
    E { g: "lines", k: "line:brann.regentTold.ilo_0020", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 29)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:24
    E { g: "lines", k: "line:brannOath.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 24)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:27
    E { g: "lines", k: "line:brannOath.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 27)] },
    // source: lore/palace.lute:20; lore/palace.lute:21
    E { g: "lines", k: "line:codexThrone.narrator_0010", e: "proven", c: &[("queries", "lore/palace.lute", 20), ("contains", "lore/palace.lute", 21)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28; lore/keepsakes.lute:14; lore/keepsakes.lute:15
    E { g: "lines", k: "line:keepLantern.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28), ("queries", "lore/keepsakes.lute", 14), ("contains", "lore/keepsakes.lute", 15)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:45; lore/keepsakes.lute:18; lore/keepsakes.lute:19
    E { g: "lines", k: "line:keepShell.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("asserts", "quests/crew.lute", 45), ("queries", "lore/keepsakes.lute", 18), ("contains", "lore/keepsakes.lute", 19)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/crown.lute:13
    E { g: "lines", k: "line:last.crown.ilo_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/crown.lute", 13)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/crown.lute:19
    E { g: "lines", k: "line:last.crown.ilo_0020", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/crown.lute", 19)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/crown.lute:14
    E { g: "lines", k: "line:last.crown.narrator_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/crown.lute", 14)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/crown.lute:20
    E { g: "lines", k: "line:last.crown.narrator_0020", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/crown.lute", 20)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/rail.lute:12
    E { g: "lines", k: "line:last.rail.brann_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/rail.lute", 12)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/rail.lute:13
    E { g: "lines", k: "line:last.rail.ilo_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/rail.lute", 13)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/rail.lute:15
    E { g: "lines", k: "line:last.rail.narrator_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/rail.lute", 15)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/rail.lute:14
    E { g: "lines", k: "line:last.rail.sefa_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/rail.lute", 14)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/throne.lute:11
    E { g: "lines", k: "line:last.throne.narrator_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/throne.lute", 11)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:19; scenes/last/throne.lute:12
    E { g: "lines", k: "line:last.throne.quill_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 19), ("contains", "scenes/last/throne.lute", 12)] },
    // source: quests/library.lute:10; quests/library.lute:15
    E { g: "lines", k: "line:libraryKey.quill_0010", e: "proven", c: &[("queries", "quests/library.lute", 10), ("contains", "quests/library.lute", 15)] },
    // source: quests/library.lute:10; quests/library.lute:18
    E { g: "lines", k: "line:libraryKey.quill_0020", e: "proven", c: &[("queries", "quests/library.lute", 10), ("contains", "quests/library.lute", 18)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:32; lore/quill.lute:38; lore/quill.lute:39
    E { g: "lines", k: "line:quillAfter.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("writes", "quests/crew.lute", 32), ("reads", "lore/quill.lute", 38), ("contains", "lore/quill.lute", 39)] },
    // source: lore/quill.lute:26; lore/quill.lute:27
    E { g: "lines", k: "line:quillHeir.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("contains", "lore/quill.lute", 27)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:37
    E { g: "lines", k: "line:quillMemory.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("contains", "quests/crew.lute", 37)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:41
    E { g: "lines", k: "line:quillMemory.quill_0020", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("contains", "quests/crew.lute", 41)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:44
    E { g: "lines", k: "line:quillMemory.quill_0030", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("contains", "quests/crew.lute", 44)] },
    // source: world.schema.yaml:65; lore/quill.lute:30; lore/quill.lute:31
    E { g: "lines", k: "line:quillWitness.quill_0010", e: "proven", c: &[("derives", "world.schema.yaml", 65), ("queries", "lore/quill.lute", 30), ("contains", "lore/quill.lute", 31)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:21
    E { g: "objectives", k: "objective:brannOath.surface", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 21)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22
    E { g: "objectives", k: "objective:brannOath.tell", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22)] },
    // source: quests/library.lute:10; quests/library.lute:13
    E { g: "objectives", k: "objective:libraryKey.keeper", e: "proven", c: &[("queries", "quests/library.lute", 10), ("contains", "quests/library.lute", 13)] },
    // source: quests/library.lute:10; quests/library.lute:12
    E { g: "objectives", k: "objective:libraryKey.sluice", e: "proven", c: &[("queries", "quests/library.lute", 10), ("contains", "quests/library.lute", 12)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:34
    E { g: "objectives", k: "objective:quillMemory.death", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("contains", "quests/crew.lute", 34)] },
    // source: lore/quill.lute:26; quests/crew.lute:35
    E { g: "objectives", k: "objective:quillMemory.heir", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35; quests/crew.lute:33
    E { g: "objectives", k: "objective:quillMemory.name", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35), ("contains", "quests/crew.lute", 33)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22
    E { g: "quests", k: "quest:brannOath", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22)] },
    // source: quests/library.lute:10
    E { g: "quests", k: "quest:libraryKey", e: "proven", c: &[("queries", "quests/library.lute", 10)] },
    // source: lore/quill.lute:26; quests/crew.lute:35; quests/crew.lute:35
    E { g: "quests", k: "quest:quillMemory", e: "proven", c: &[("queries", "lore/quill.lute", 26), ("discloses", "quests/crew.lute", 35), ("completes", "quests/crew.lute", 35)] },
    // source: lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:20
    E { g: "rewards", k: "reward:brannOath#0", e: "proven", c: &[("queries", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 20)] },
    // source: quests/library.lute:10; quests/library.lute:11
    E { g: "rewards", k: "reward:libraryKey#0", e: "proven", c: &[("queries", "quests/library.lute", 10), ("contains", "quests/library.lute", 11)] },
];

const SLEW: &[E] = &[
    // source: scenes/hub/regent-fell.lute:9
    E { g: "beats", k: "scene:hub.regentFell", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9
    E { g: "disclosures", k: "beat:bonds.brann1", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:14
    E { g: "disclosures", k: "beat:bonds.brann2", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 14)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26
    E { g: "disclosures", k: "beat:brann.regentTold", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28; lore/keepsakes.lute:14
    E { g: "disclosures", k: "entry:keepLantern", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28), ("queries", "lore/keepsakes.lute", 14)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64; lore/quill.lute:30
    E { g: "disclosures", k: "entry:quillWitness", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64), ("queries", "lore/quill.lute", 30)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28
    E { g: "downstream", k: "fact:gifted(brann,lantern)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:63
    E { g: "downstream", k: "fact:heard(brann,regent)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 63)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(quill,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(sefa,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(tavi,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31
    E { g: "downstream", k: "fact:told(brann,regent)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; world.schema.yaml:67
    E { g: "downstream", k: "fact:trusts(brann)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("derives", "world.schema.yaml", 67)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:19
    E { g: "downstream", k: "state:quest.brannOath.state", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("writes", "quests/crew.lute", 19)] },
    // source: quests/dive.lute:11; quests/dive.lute:11; quests/dive.lute:9
    E { g: "downstream", k: "state:quest.regentHunt.state", e: "proven", c: &[("queries", "quests/dive.lute", 11), ("completes", "quests/dive.lute", 11), ("writes", "quests/dive.lute", 9)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32
    E { g: "downstream", k: "state:user.bond.brann", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9; lore/bonds.lute:10
    E { g: "lines", k: "line:bonds.brann1.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9), ("contains", "lore/bonds.lute", 10)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:9; lore/bonds.lute:11
    E { g: "lines", k: "line:bonds.brann1.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 9), ("contains", "lore/bonds.lute", 11)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:32; lore/bonds.lute:14; lore/bonds.lute:15
    E { g: "lines", k: "line:bonds.brann2.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("writes", "lore/brann.lute", 32), ("reads", "lore/bonds.lute", 14), ("contains", "lore/bonds.lute", 15)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:28
    E { g: "lines", k: "line:brann.regentTold.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 28)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:30
    E { g: "lines", k: "line:brann.regentTold.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 30)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:27
    E { g: "lines", k: "line:brann.regentTold.ilo_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 27)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:29
    E { g: "lines", k: "line:brann.regentTold.ilo_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("contains", "lore/brann.lute", 29)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:24
    E { g: "lines", k: "line:brannOath.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 24)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:27
    E { g: "lines", k: "line:brannOath.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 27)] },
    // source: scenes/hub/regent-fell.lute:9; scenes/hub/regent-fell.lute:18
    E { g: "lines", k: "line:hub.regentFell.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("contains", "scenes/hub/regent-fell.lute", 18)] },
    // source: scenes/hub/regent-fell.lute:9; scenes/hub/regent-fell.lute:20
    E { g: "lines", k: "line:hub.regentFell.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("contains", "scenes/hub/regent-fell.lute", 20)] },
    // source: scenes/hub/regent-fell.lute:9; scenes/hub/regent-fell.lute:19
    E { g: "lines", k: "line:hub.regentFell.ilo_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("contains", "scenes/hub/regent-fell.lute", 19)] },
    // source: scenes/hub/regent-fell.lute:9; scenes/hub/regent-fell.lute:16
    E { g: "lines", k: "line:hub.regentFell.narrator_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("contains", "scenes/hub/regent-fell.lute", 16)] },
    // source: scenes/hub/regent-fell.lute:9; scenes/hub/regent-fell.lute:17
    E { g: "lines", k: "line:hub.regentFell.sefa_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("contains", "scenes/hub/regent-fell.lute", 17)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:28; lore/keepsakes.lute:14; lore/keepsakes.lute:15
    E { g: "lines", k: "line:keepLantern.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("asserts", "quests/crew.lute", 28), ("queries", "lore/keepsakes.lute", 14), ("contains", "lore/keepsakes.lute", 15)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; world.schema.yaml:64; lore/quill.lute:30; lore/quill.lute:31
    E { g: "lines", k: "line:quillWitness.quill_0010", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 64), ("queries", "lore/quill.lute", 30), ("contains", "lore/quill.lute", 31)] },
    // source: quests/dive.lute:11; quests/dive.lute:11; quests/dive.lute:13
    E { g: "lines", k: "line:regentHunt.quill_0010", e: "proven", c: &[("queries", "quests/dive.lute", 11), ("completes", "quests/dive.lute", 11), ("contains", "quests/dive.lute", 13)] },
    // source: quests/dive.lute:11; quests/dive.lute:11; quests/dive.lute:16
    E { g: "lines", k: "line:regentHunt.quill_0020", e: "proven", c: &[("queries", "quests/dive.lute", 11), ("completes", "quests/dive.lute", 11), ("contains", "quests/dive.lute", 16)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:21
    E { g: "objectives", k: "objective:brannOath.surface", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 21)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22
    E { g: "objectives", k: "objective:brannOath.tell", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22)] },
    // source: quests/dive.lute:11
    E { g: "objectives", k: "objective:regentHunt.slay", e: "proven", c: &[("queries", "quests/dive.lute", 11)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22
    E { g: "quests", k: "quest:brannOath", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22)] },
    // source: quests/dive.lute:11; quests/dive.lute:11
    E { g: "quests", k: "quest:regentHunt", e: "proven", c: &[("queries", "quests/dive.lute", 11), ("completes", "quests/dive.lute", 11)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:26; lore/brann.lute:31; quests/crew.lute:22; quests/crew.lute:22; quests/crew.lute:20
    E { g: "rewards", k: "reward:brannOath#0", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 26), ("asserts", "lore/brann.lute", 31), ("queries", "quests/crew.lute", 22), ("completes", "quests/crew.lute", 22), ("contains", "quests/crew.lute", 20)] },
    // source: quests/dive.lute:11; quests/dive.lute:11; quests/dive.lute:10
    E { g: "rewards", k: "reward:regentHunt#0", e: "proven", c: &[("queries", "quests/dive.lute", 11), ("completes", "quests/dive.lute", 11), ("contains", "quests/dive.lute", 10)] },
];

fn run(target: &str, json: bool) -> String {
    let mut cmd = Command::new(BIN);
    cmd.args(["impact", ROOT, target]);
    if json { cmd.arg("--json"); }
    let out = cmd.output().expect("lute binary");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf8")
}

fn check(target: &str, expected: &[E]) {
    let root: Value = serde_json::from_str(&run(target, true)).expect("impact JSON");
    assert!(root["root"].as_str().unwrap().ends_with("/docs/examples/games/drowned-crown"));
    let items = root["items"].as_object().expect("items object");
    let mut got = Vec::new();
    for (g, rows) in items {
        for row in rows.as_array().unwrap() {
            let key = row["key"].as_str().unwrap();
            let evidence = row["evidence"].as_str().unwrap();
            let reasons = row["reasons"].as_array().unwrap();
            let chain: Vec<_> = reasons.iter().map(|r| (r["edge"].as_str().unwrap().to_string(), r["file"].as_str().unwrap().to_string(), r["line"].as_u64().unwrap() as u32)).collect();
            got.push((g.clone(), key.to_string(), evidence.to_string(), chain));
        }
    }
    got.sort_by(|a,b| (a.0.as_str(),a.1.as_str()).cmp(&(b.0.as_str(),b.1.as_str())));
    let mut want: Vec<_> = expected.iter().map(|e| (e.g.to_string(),e.k.to_string(),e.e.to_string(),e.c.iter().map(|(a,b,c)|(a.to_string(),b.to_string(),*c)).collect::<Vec<_>>())).collect();
    want.sort_by(|a,b| (a.0.as_str(),a.1.as_str()).cmp(&(b.0.as_str(),b.1.as_str())));
    assert_eq!(got, want);
}

#[test]
fn felled_golden() {
    check("fact:felled(regent)", FELLED);
    let json = run("fact:felled(regent)", true);
    let report: Value = serde_json::from_str(&json).unwrap();
    let line = report["items"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "line:bonds.brann1.brann_0010")
        .expect("pinned line item");
    assert_eq!(line["lineId"], "bonds.brann1.brann_0010");
    assert_eq!(line["speaker"], "brann");
    assert_eq!(line["file"], "lore/bonds.lute");
    assert_eq!(line["line"], 10);
    let quest = report["items"]["quests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "quest:brannOath")
        .expect("pinned quest item");
    assert_eq!(quest["file"], "quests/crew.lute");
    assert_eq!(quest["line"], 19);
    for forbidden in ["scenes/talk/sefa-eel.lute", "codexHall", "codexChoir", "bossDefeated", "guardianAgain", "anyGuardian", "eelFirst", "choirFirst"] {
        assert!(!json.contains(forbidden), "forbidden {forbidden}");
    }
    let text = run("fact:felled(regent)", false);
    assert!(text.contains("beats\n"));
    assert!(text.contains("    - queries: holds('felled', ['regent']) && !holds('told', ['brann', 'regent']) (lore/brann.lute:26)"));
}

#[test]
fn slew_golden_and_negatives() {
    check("fact:slew(regent)", SLEW);
    let json = run("fact:slew(regent)", true);
    for forbidden in ["scenes/talk/sefa-eel.lute", "codexHall", "codexChoir", "bossDefeated", "guardianAgain", "anyGuardian", "eelFirst", "choirFirst", "libraryKey", "quillHeir", "codexThrone", "@readyForCrown", "readyForCrown"] {
        assert!(!json.contains(forbidden), "forbidden {forbidden}");
    }
}

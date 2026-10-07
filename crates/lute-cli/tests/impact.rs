use std::process::Command;
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples/games/drowned-crown");

struct E { g: &'static str, k: &'static str, e: &'static str, c: &'static [(&'static str, &'static str, u32)] }

const FELLED: &[E] = &[
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13
    E { g: "beats", k: "occasion:lastDive", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21
    E { g: "beats", k: "scene:last.crown", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21
    E { g: "beats", k: "scene:last.rail", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21
    E { g: "beats", k: "scene:last.throne", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8
    E { g: "disclosures", k: "beat:bonds.brann1", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13
    E { g: "disclosures", k: "beat:bonds.brann2", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:37; lore/bonds.lute:25
    E { g: "disclosures", k: "beat:bonds.quill1", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 37), ("reads", "lore/bonds.lute", 25)] },
    // source: lore/brann.lute:25
    E { g: "disclosures", k: "beat:brann.regentTold", e: "proven", c: &[("queries", "lore/brann.lute", 25)] },
    // source: lore/palace.lute:20
    E { g: "disclosures", k: "entry:codexThrone", e: "proven", c: &[("queries", "lore/palace.lute", 20)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27; lore/keepsakes.lute:13
    E { g: "disclosures", k: "entry:keepLantern", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27), ("queries", "lore/keepsakes.lute", 13)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:44; lore/keepsakes.lute:17
    E { g: "disclosures", k: "entry:keepShell", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("asserts", "quests/crew.lute", 44), ("queries", "lore/keepsakes.lute", 17)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:31; lore/quill.lute:37
    E { g: "disclosures", k: "entry:quillAfter", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 31), ("reads", "lore/quill.lute", 37)] },
    // source: lore/quill.lute:25
    E { g: "disclosures", k: "entry:quillHeir", e: "proven", c: &[("queries", "lore/quill.lute", 25)] },
    // source: world.schema.yaml:65; lore/quill.lute:29
    E { g: "disclosures", k: "entry:quillWitness", e: "proven", c: &[("derives", "world.schema.yaml", 65), ("queries", "lore/quill.lute", 29)] },
    // source: world.schema.yaml:77
    E { g: "downstream", k: "def:readyForCrown", e: "proven", c: &[("queries", "world.schema.yaml", 77)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27
    E { g: "downstream", k: "fact:gifted(brann,lantern)", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:44
    E { g: "downstream", k: "fact:gifted(quill,shell)", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("asserts", "quests/crew.lute", 44)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:63
    E { g: "downstream", k: "fact:heard(brann,regent)", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 63)] },
    // source: world.schema.yaml:65
    E { g: "downstream", k: "fact:heard(quill,regent)", e: "proven", c: &[("derives", "world.schema.yaml", 65)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(sefa,regent)", e: "heuristic", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(tavi,regent)", e: "heuristic", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64)] },
    // source: lore/brann.lute:25; lore/brann.lute:30
    E { g: "downstream", k: "fact:told(brann,regent)", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; world.schema.yaml:67
    E { g: "downstream", k: "fact:trusts(brann)", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 67)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:18
    E { g: "downstream", k: "state:quest.brannOath.state", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("writes", "quests/crew.lute", 18)] },
    // source: quests/library.lute:9; quests/library.lute:9
    E { g: "downstream", k: "state:quest.libraryKey.state", e: "proven", c: &[("queries", "quests/library.lute", 9), ("writes", "quests/library.lute", 9)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:31
    E { g: "downstream", k: "state:quest.quillMemory.state", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 31)] },
    // source: lore/brann.lute:25; lore/brann.lute:31
    E { g: "downstream", k: "state:user.bond.brann", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:37
    E { g: "downstream", k: "state:user.bond.quill", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 37)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:8
    E { g: "lines", k: "line:bonds.brann1.bondStory#use-001.narrator_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 8)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:9
    E { g: "lines", k: "line:bonds.brann1.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 9)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:10
    E { g: "lines", k: "line:bonds.brann1.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 10)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13; lore/bonds.lute:13
    E { g: "lines", k: "line:bonds.brann2.bondStory#use-002.narrator_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13), ("contains", "lore/bonds.lute", 13)] },
    // source: lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13; lore/bonds.lute:14
    E { g: "lines", k: "line:bonds.brann2.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13), ("contains", "lore/bonds.lute", 14)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:37; lore/bonds.lute:25; lore/bonds.lute:25
    E { g: "lines", k: "line:bonds.quill1.bondStory#use-005.narrator_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 37), ("reads", "lore/bonds.lute", 25), ("contains", "lore/bonds.lute", 25)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:37; lore/bonds.lute:25; lore/bonds.lute:26
    E { g: "lines", k: "line:bonds.quill1.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 37), ("reads", "lore/bonds.lute", 25), ("contains", "lore/bonds.lute", 26)] },
    // source: lore/brann.lute:25; lore/brann.lute:27
    E { g: "lines", k: "line:brann.regentTold.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 27)] },
    // source: lore/brann.lute:25; lore/brann.lute:29
    E { g: "lines", k: "line:brann.regentTold.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 29)] },
    // source: lore/brann.lute:25; lore/brann.lute:26
    E { g: "lines", k: "line:brann.regentTold.ilo_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 26)] },
    // source: lore/brann.lute:25; lore/brann.lute:28
    E { g: "lines", k: "line:brann.regentTold.ilo_0020", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 28)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:23
    E { g: "lines", k: "line:brannOath.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 23)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:26
    E { g: "lines", k: "line:brannOath.brann_0020", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 26)] },
    // source: lore/palace.lute:20; lore/palace.lute:21
    E { g: "lines", k: "line:codexThrone.narrator_0010", e: "proven", c: &[("queries", "lore/palace.lute", 20), ("contains", "lore/palace.lute", 21)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27; lore/keepsakes.lute:13; lore/keepsakes.lute:14
    E { g: "lines", k: "line:keepLantern.brann_0010", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27), ("queries", "lore/keepsakes.lute", 13), ("contains", "lore/keepsakes.lute", 14)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:44; lore/keepsakes.lute:17; lore/keepsakes.lute:18
    E { g: "lines", k: "line:keepShell.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("asserts", "quests/crew.lute", 44), ("queries", "lore/keepsakes.lute", 17), ("contains", "lore/keepsakes.lute", 18)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/rail.lute:12
    E { g: "lines", k: "line:last.rail.brann_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/rail.lute", 12)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/rail.lute:13
    E { g: "lines", k: "line:last.rail.ilo_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/rail.lute", 13)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/rail.lute:15
    E { g: "lines", k: "line:last.rail.narrator_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/rail.lute", 15)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/rail.lute:14
    E { g: "lines", k: "line:last.rail.sefa_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/rail.lute", 14)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/throne.lute:11
    E { g: "lines", k: "line:last.throne.narrator_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/throne.lute", 11)] },
    // source: world.schema.yaml:77; plugins/game.occasions/occasions/game.yaml:13; lute.project.yaml:21; scenes/last/throne.lute:12
    E { g: "lines", k: "line:last.throne.quill_0010", e: "proven", c: &[("queries", "world.schema.yaml", 77), ("gates", "plugins/game.occasions/occasions/game.yaml", 13), ("raises", "lute.project.yaml", 22), ("contains", "scenes/last/throne.lute", 12)] },
    // source: quests/library.lute:9; quests/library.lute:14
    E { g: "lines", k: "line:libraryKey.quill_0010", e: "proven", c: &[("queries", "quests/library.lute", 9), ("contains", "quests/library.lute", 14)] },
    // source: quests/library.lute:9; quests/library.lute:17
    E { g: "lines", k: "line:libraryKey.quill_0020", e: "proven", c: &[("queries", "quests/library.lute", 9), ("contains", "quests/library.lute", 17)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:31; lore/quill.lute:37; lore/quill.lute:38
    E { g: "lines", k: "line:quillAfter.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("writes", "quests/crew.lute", 31), ("reads", "lore/quill.lute", 37), ("contains", "lore/quill.lute", 38)] },
    // source: lore/quill.lute:25; lore/quill.lute:26
    E { g: "lines", k: "line:quillHeir.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("contains", "lore/quill.lute", 26)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:36
    E { g: "lines", k: "line:quillMemory.quill_0010", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("contains", "quests/crew.lute", 36)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:40
    E { g: "lines", k: "line:quillMemory.quill_0020", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("contains", "quests/crew.lute", 40)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:43
    E { g: "lines", k: "line:quillMemory.quill_0030", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("contains", "quests/crew.lute", 43)] },
    // source: world.schema.yaml:65; lore/quill.lute:29; lore/quill.lute:30
    E { g: "lines", k: "line:quillWitness.quill_0010", e: "proven", c: &[("derives", "world.schema.yaml", 65), ("queries", "lore/quill.lute", 29), ("contains", "lore/quill.lute", 30)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:20
    E { g: "objectives", k: "objective:brannOath.surface", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 20)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21
    E { g: "objectives", k: "objective:brannOath.tell", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21)] },
    // source: quests/library.lute:9; quests/library.lute:12
    E { g: "objectives", k: "objective:libraryKey.keeper", e: "proven", c: &[("queries", "quests/library.lute", 9), ("contains", "quests/library.lute", 12)] },
    // source: quests/library.lute:9; quests/library.lute:11
    E { g: "objectives", k: "objective:libraryKey.sluice", e: "proven", c: &[("queries", "quests/library.lute", 9), ("contains", "quests/library.lute", 11)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:33
    E { g: "objectives", k: "objective:quillMemory.death", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("contains", "quests/crew.lute", 33)] },
    // source: lore/quill.lute:25; quests/crew.lute:34
    E { g: "objectives", k: "objective:quillMemory.heir", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34; quests/crew.lute:32
    E { g: "objectives", k: "objective:quillMemory.name", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34), ("contains", "quests/crew.lute", 32)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21
    E { g: "quests", k: "quest:brannOath", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21)] },
    // source: quests/library.lute:9
    E { g: "quests", k: "quest:libraryKey", e: "proven", c: &[("queries", "quests/library.lute", 9)] },
    // source: lore/quill.lute:25; quests/crew.lute:34; quests/crew.lute:34
    E { g: "quests", k: "quest:quillMemory", e: "proven", c: &[("queries", "lore/quill.lute", 25), ("discloses", "quests/crew.lute", 34), ("completes", "quests/crew.lute", 34)] },
    // source: lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:19
    E { g: "rewards", k: "reward:brannOath#0", e: "proven", c: &[("queries", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 19)] },
    // source: quests/library.lute:9; quests/library.lute:10
    E { g: "rewards", k: "reward:libraryKey#0", e: "proven", c: &[("queries", "quests/library.lute", 9), ("contains", "quests/library.lute", 10)] },
];

const SLEW: &[E] = &[
    // source: scenes/hub/regent-fell.lute:9
    E { g: "beats", k: "scene:hub.regentFell", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8
    E { g: "disclosures", k: "beat:bonds.brann1", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13
    E { g: "disclosures", k: "beat:bonds.brann2", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25
    E { g: "disclosures", k: "beat:brann.regentTold", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27; lore/keepsakes.lute:13
    E { g: "disclosures", k: "entry:keepLantern", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27), ("queries", "lore/keepsakes.lute", 13)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64; lore/quill.lute:29
    E { g: "disclosures", k: "entry:quillWitness", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64), ("queries", "lore/quill.lute", 29)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27
    E { g: "downstream", k: "fact:gifted(brann,lantern)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:63
    E { g: "downstream", k: "fact:heard(brann,regent)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 63)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(quill,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(sefa,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64
    E { g: "downstream", k: "fact:heard(tavi,regent)", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30
    E { g: "downstream", k: "fact:told(brann,regent)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; world.schema.yaml:67
    E { g: "downstream", k: "fact:trusts(brann)", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("derives", "world.schema.yaml", 67)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:18
    E { g: "downstream", k: "state:quest.brannOath.state", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("writes", "quests/crew.lute", 18)] },
    // source: quests/dive.lute:10; quests/dive.lute:10; quests/dive.lute:8
    E { g: "downstream", k: "state:quest.regentHunt.state", e: "proven", c: &[("queries", "quests/dive.lute", 10), ("completes", "quests/dive.lute", 10), ("writes", "quests/dive.lute", 8)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31
    E { g: "downstream", k: "state:user.bond.brann", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:8
    E { g: "lines", k: "line:bonds.brann1.bondStory#use-001.narrator_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 8)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:9
    E { g: "lines", k: "line:bonds.brann1.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 9)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:8; lore/bonds.lute:10
    E { g: "lines", k: "line:bonds.brann1.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 8), ("contains", "lore/bonds.lute", 10)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13; lore/bonds.lute:13
    E { g: "lines", k: "line:bonds.brann2.bondStory#use-002.narrator_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13), ("contains", "lore/bonds.lute", 13)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:31; lore/bonds.lute:13; lore/bonds.lute:14
    E { g: "lines", k: "line:bonds.brann2.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("writes", "lore/brann.lute", 31), ("reads", "lore/bonds.lute", 13), ("contains", "lore/bonds.lute", 14)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:27
    E { g: "lines", k: "line:brann.regentTold.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 27)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:29
    E { g: "lines", k: "line:brann.regentTold.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 29)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:26
    E { g: "lines", k: "line:brann.regentTold.ilo_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 26)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:28
    E { g: "lines", k: "line:brann.regentTold.ilo_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("contains", "lore/brann.lute", 28)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:23
    E { g: "lines", k: "line:brannOath.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 23)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:26
    E { g: "lines", k: "line:brannOath.brann_0020", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 26)] },
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
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:27; lore/keepsakes.lute:13; lore/keepsakes.lute:14
    E { g: "lines", k: "line:keepLantern.brann_0010", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("asserts", "quests/crew.lute", 27), ("queries", "lore/keepsakes.lute", 13), ("contains", "lore/keepsakes.lute", 14)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; world.schema.yaml:64; lore/quill.lute:29; lore/quill.lute:30
    E { g: "lines", k: "line:quillWitness.quill_0010", e: "heuristic", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("derives", "world.schema.yaml", 64), ("queries", "lore/quill.lute", 29), ("contains", "lore/quill.lute", 30)] },
    // source: quests/dive.lute:10; quests/dive.lute:10; quests/dive.lute:12
    E { g: "lines", k: "line:regentHunt.quill_0010", e: "proven", c: &[("queries", "quests/dive.lute", 10), ("completes", "quests/dive.lute", 10), ("contains", "quests/dive.lute", 12)] },
    // source: quests/dive.lute:10; quests/dive.lute:10; quests/dive.lute:15
    E { g: "lines", k: "line:regentHunt.quill_0020", e: "proven", c: &[("queries", "quests/dive.lute", 10), ("completes", "quests/dive.lute", 10), ("contains", "quests/dive.lute", 15)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:20
    E { g: "objectives", k: "objective:brannOath.surface", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 20)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21
    E { g: "objectives", k: "objective:brannOath.tell", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21)] },
    // source: quests/dive.lute:10
    E { g: "objectives", k: "objective:regentHunt.slay", e: "proven", c: &[("queries", "quests/dive.lute", 10)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21
    E { g: "quests", k: "quest:brannOath", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21)] },
    // source: quests/dive.lute:10; quests/dive.lute:10
    E { g: "quests", k: "quest:regentHunt", e: "proven", c: &[("queries", "quests/dive.lute", 10), ("completes", "quests/dive.lute", 10)] },
    // source: scenes/hub/regent-fell.lute:9; lore/brann.lute:25; lore/brann.lute:30; quests/crew.lute:21; quests/crew.lute:21; quests/crew.lute:19
    E { g: "rewards", k: "reward:brannOath#0", e: "proven", c: &[("gates", "scenes/hub/regent-fell.lute", 9), ("gates", "lore/brann.lute", 25), ("asserts", "lore/brann.lute", 30), ("queries", "quests/crew.lute", 21), ("completes", "quests/crew.lute", 21), ("contains", "quests/crew.lute", 19)] },
    // source: quests/dive.lute:10; quests/dive.lute:10; quests/dive.lute:9
    E { g: "rewards", k: "reward:regentHunt#0", e: "proven", c: &[("queries", "quests/dive.lute", 10), ("completes", "quests/dive.lute", 10), ("contains", "quests/dive.lute", 9)] },
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
    assert_eq!(line["line"], 9);
    let quest = report["items"]["quests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "quest:brannOath")
        .expect("pinned quest item");
    assert_eq!(quest["file"], "quests/crew.lute");
    assert_eq!(quest["line"], 18);
    for forbidden in ["scenes/talk/sefa-eel.lute", "codexHall", "codexChoir", "bossDefeated", "guardianAgain", "anyGuardian", "eelFirst", "choirFirst"] {
        assert!(!json.contains(forbidden), "forbidden {forbidden}");
    }
    let text = run("fact:felled(regent)", false);
    assert!(text.contains("beats\n"));
    assert!(text.contains("    - queries: holds('felled', ['regent']) && !holds('told', ['brann', 'regent']) (lore/brann.lute:25)"));
}

#[test]
fn slew_golden_and_negatives() {
    check("fact:slew(regent)", SLEW);
    let json = run("fact:slew(regent)", true);
    for forbidden in ["scenes/talk/sefa-eel.lute", "codexHall", "codexChoir", "bossDefeated", "guardianAgain", "anyGuardian", "eelFirst", "choirFirst", "libraryKey", "quillHeir", "codexThrone", "@readyForCrown", "readyForCrown"] {
        assert!(!json.contains(forbidden), "forbidden {forbidden}");
    }
}

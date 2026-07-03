//! Authored `assetId` acceptance (plugin §6.9): the on-disk `idola.minigame`
//! fixture exports an `MG` assetKind (`assetkinds/art.yaml`) and a `::mgart`
//! directive whose `art` attr is typed `assetKind(MG)`. Under `--project`, a
//! scene with a VALID authored id (`MG.bianca_service_01.0`) checks clean; a bad
//! provider segment (`MG.nope.0`, an absent `minigameId`) yields
//! `E-ASSET-SEGMENT` and exits 1. Catalog `catalog/` auto-discovers under
//! `--project`, so the valid gameId resolves Fresh and the bad one Absent.

use std::process::Command;

fn lute_bin() -> &'static str {
    env!("CARGO_BIN_EXE_lute")
}

#[test]
fn mgart_valid_is_clean() {
    let out = Command::new(lute_bin())
        .args([
            "check",
            "../../docs/examples/idola-portrait.lute",
            "--project",
            "../../docs/examples/idola-project",
            "--json",
        ])
        .output()
        .expect("run lute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"ok\": true"),
        "valid authored MG id -> ok:true; got {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !stdout.contains("E-ASSET-"),
        "no asset diagnostics expected; got {stdout}"
    );
    assert_eq!(out.status.code(), Some(0), "exit 0 on clean");
}

#[test]
fn mgart_bad_segment_flags() {
    // A temp scene identical to idola-portrait.lute EXCEPT an absent minigameId
    // in the gameId segment. Per-segment validation must raise E-ASSET-SEGMENT.
    let tmp = std::env::temp_dir().join(format!("lute_mgart_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let scene = tmp.join("idola-portrait-bad.lute");
    std::fs::write(
        &scene,
        "---\n\
         character: bianca\n\
         season: 1\n\
         episode: 5\n\
         luteVersion: \"0.0.1\"\n\
         profile: date-minigame\n\
         plugins:\n\
         \x20\x20idola.minigame:\n\
         \x20\x20\x20\x20resultScope: scene\n\
         \x20\x20\x20\x20allowedKinds: [rhythm]\n\
         ---\n\
         \n\
         # Portrait asset demo\n\
         \n\
         ## Shot 1.\n\
         \n\
         ::mgart{art=\"MG.nope.0\"}\n",
    )
    .unwrap();

    let out = Command::new(lute_bin())
        .args([
            "check",
            scene.to_str().unwrap(),
            "--project",
            "../../docs/examples/idola-project",
            "--json",
        ])
        .output()
        .expect("run lute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E-ASSET-SEGMENT"),
        "absent gameId segment must raise E-ASSET-SEGMENT; got {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "exit 1 on asset segment error");
    std::fs::remove_dir_all(&tmp).ok();
}

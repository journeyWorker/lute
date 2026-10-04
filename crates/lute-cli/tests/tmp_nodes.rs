use lute_model::{ModelOptions,ProjectModel};
#[test] fn dump(){ let root=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/games/monster-league"); let m=ProjectModel::build_single_root(&root,&ModelOptions::default()).unwrap(); for (k,n) in &m.graph().nodes { if k.key.contains("morwen") { println!("NODE {} {:?} {:?}",k.canonical(),n.file,n.span); } } }

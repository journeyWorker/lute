use std::path::{Path, PathBuf};
use std::process::Command;
use lute_cli::build_runtime;
use lute_runtime::runtime::{Input, Seed};
use serde_json::Value;

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/session") }
fn cases() -> Vec<PathBuf> { let mut out=Vec::new(); for e in std::fs::read_dir(root()).unwrap() { let p=e.unwrap().path(); if p.join("project/script.play.yaml").is_file(){out.push(p)} } out.sort(); out }
fn lines(path:&Path)->Vec<Value>{std::fs::read_to_string(path).unwrap().lines().map(|x|serde_json::from_str(x).unwrap()).collect()}
#[test]
fn session_event_streams_and_runtime_replay_match() {
 for case in cases() {
  let expected_path=case.join("expected.jsonl"); let expected=std::fs::read_to_string(&expected_path).unwrap();
  let got=Command::new(env!("CARGO_BIN_EXE_lute")).args(["play",case.join("project").to_str().unwrap(),"--script",case.join("project/script.play.yaml").to_str().unwrap(),"--events"]).output().unwrap();
  assert!(got.status.success() || got.status.code()==Some(3),"{} failed: {}",case.display(),String::from_utf8_lossy(&got.stderr));
  assert_eq!(String::from_utf8(got.stdout).unwrap(),expected,"{} CLI stream",case.display());
  let ins=lines(&case.join("inputs.jsonl")); let outs=lines(&expected_path); let seed:Seed=serde_json::from_value(ins[0]["seed"].clone()).unwrap();
  let runtime=build_runtime(&case.join("project")).unwrap(); let (mut state,output)=runtime.begin(seed).unwrap(); assert_eq!(serde_json::to_value(output).unwrap(),outs[0]["output"],"{} begin",case.display());
  for (i,line) in ins.iter().skip(1).enumerate() { let input:Input=serde_json::from_value(line["input"].clone()).unwrap(); let (next,out)=match runtime.step(state,input) { Ok(v)=>v, Err((_s,e))=>panic!("{} step {} rejected {} {}",case.display(),i,e.code,e.message) }; state=next; assert_eq!(serde_json::to_value(out).unwrap(),outs[i+1]["output"],"{} step {}",case.display(),i); }
 }
}

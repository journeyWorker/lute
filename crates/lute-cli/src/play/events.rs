use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lute_runtime::runtime::{AdvanceBy, Await, Input, PickInput, Seed, StateWrite, WritesInput};
use lute_runtime::Runtime;
use serde_json::Value;

#[derive(Clone)]
pub struct ScriptStep { pub input: Input, pub choose: BTreeMap<String, Vec<String>>, pub bridges: BTreeMap<String, Vec<BTreeMap<String, Value>>> }

fn json_yaml(text: &str) -> Result<Value, String> { serde_json::to_value(serde_yaml::from_str::<serde_yaml::Value>(text).map_err(|e| e.to_string())?).map_err(|e| e.to_string()) }
fn scalar(v: &Value) -> Result<Value, String> { match v { Value::Null|Value::Bool(_)|Value::Number(_)|Value::String(_) => Ok(v.clone()), _ => Err(format!("non-scalar value {v}")) } }
fn writes(value: Option<&Value>) -> Result<WritesInput, String> {
    let Some(Value::Object(map)) = value else { return Ok(WritesInput::default()) };
    let mut out = WritesInput::default();
    if let Some(Value::Object(state)) = map.get("state") { for (path, value) in state { if let Value::Object(m)=value { if let Some(add)=m.get("add") { out.state.push(StateWrite{path:path.clone(),value:None,add:add.as_f64()}); } else { out.state.push(StateWrite{path:path.clone(),value:Some(scalar(value)?),add:None}); } } else { out.state.push(StateWrite{path:path.clone(),value:Some(scalar(value)?),add:None}); } } }
    for (key,dest) in [("facts",&mut out.facts),("retract",&mut out.retract),("accept",&mut out.accept)] { if let Some(Value::Array(a))=map.get(key) { dest.extend(a.iter().filter_map(Value::as_str).map(str::to_owned)); } }
    Ok(out)
}
fn pick(value: Option<&Value>) -> Result<Option<PickInput>, String> { let Some(v)=value else{return Ok(None)}; if v.as_str()==Some("none"){return Ok(Some(PickInput::Pass("pass".into())))}; Ok(Some(PickInput::Beat{beat:v.as_str().ok_or_else(||format!("invalid pick {v}"))?.into()})) }
fn choose_map(value: Option<&Value>) -> BTreeMap<String,Vec<String>> { let Some(Value::Object(m))=value else{return BTreeMap::new()}; m.iter().filter_map(|(id,v)| { let x=match v {Value::Array(a)=>a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),Value::String(s)=>vec![s.clone()], _=>return None}; Some((id.clone(),x)) }).collect() }
fn bridges(value: Option<&Value>) -> BTreeMap<String,Vec<BTreeMap<String,Value>>> { let Some(Value::Object(m))=value else{return BTreeMap::new()}; m.iter().map(|(tag,v)| { let x=match v {Value::Array(a)=>a.iter().filter_map(|i|i.as_object().cloned().map(|o|o.into_iter().collect())).collect(),Value::Object(o)=>vec![o.clone().into_iter().collect()], _=>Vec::new()}; (tag.clone(),x) }).collect() }
fn advance(v:&Value)->Result<AdvanceBy,String>{match v {Value::String(s)=>Ok(AdvanceBy::Named(s.clone())),Value::Number(n)=>Ok(AdvanceBy::Slots(n.as_u64().ok_or_else(||format!("invalid advance {v}"))? as u32)),Value::Object(m)=>{let to=m.get("to").ok_or_else(||format!("invalid advance {v}"))?;if let Some(s)=to.as_str(){Ok(AdvanceBy::Named(s.into()))}else{let o=to.as_object().ok_or_else(||format!("invalid advance {v}"))?;Ok(AdvanceBy::To{to:lute_runtime::runtime::ClockPosition{weekday:o.get("weekday").and_then(Value::as_str).ok_or("missing weekday")?.into(),slot:o.get("slot").and_then(Value::as_str).ok_or("missing slot")?.into()}})}},_=>Err(format!("invalid advance {v}"))}}

pub fn parse_script(text:&str, base:Option<&Path>)->Result<(Seed,Vec<ScriptStep>,BTreeMap<String,Vec<String>>,BTreeMap<String,Vec<BTreeMap<String,Value>>>),String>{
 let root=json_yaml(text)?.as_object().cloned().ok_or("script is not a map")?;
 let state=root.get("state").and_then(Value::as_object).map(|m|m.iter().map(|(p,v)|Ok(StateWrite{path:p.clone(),value:Some(scalar(v)?),add:None})).collect::<Result<Vec<_>,String>>()).transpose()?.unwrap_or_default();
 let mut seed=Seed{state,facts:root.get("facts").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default(),derive:root.get("derive").and_then(Value::as_bool).unwrap_or(true),save:Default::default()};
 seed.save.visited=root.get("visited").and_then(Value::as_array).map(|a|a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
 seed.save.presented=root.get("presented").and_then(Value::as_object).map(|m|m.iter().map(|(k,v)|(k.clone(),v.as_array().map(|a|a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default())).collect()).unwrap_or_default();
 seed.save.quests=root.get("quests").and_then(Value::as_object).map(|m|m.iter().filter_map(|(k,v)|v.as_str().map(|s|(k.clone(),s.into()))).collect()).unwrap_or_default();
 seed.save.quest_instances=root.get("questInstances").and_then(Value::as_object).map(|m|m.iter().filter_map(|(k,v)|v.as_u64().map(|n|(k.clone(),n))).collect()).unwrap_or_default();
 seed.save.entries_read=root.get("entriesRead").and_then(Value::as_object).map(|m|m.iter().map(|(k,v)|(k.clone(),v.as_array().map(|a|a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default())).collect()).unwrap_or_default();
 let top_choose=choose_map(root.get("choose")); let top_bridges=bridges(root.get("bridges")); let arr=root.get("steps").and_then(Value::as_array).ok_or("script has no steps")?; let mut raw=Vec::new();
 for step in arr { let m=step.as_object().ok_or("step is not a map")?; if let Some(include)=m.get("include").and_then(Value::as_str) { let b=base.ok_or("include is not representable")?; let iv=json_yaml(&fs::read_to_string(b.join(include)).map_err(|e|e.to_string())?)?; for item in iv.get("steps").and_then(Value::as_array).ok_or("included file has no steps")? { let mut x=item.as_object().cloned().ok_or("included step is not a map")?; for key in ["choose","bridges"] {if let Some(v)=m.get(key){x.entry(key).or_insert_with(||v.clone());}} raw.push(Value::Object(x)); } } else {raw.push(step.clone())} }
 let mut steps=Vec::new(); for step in raw { let m=step.as_object().ok_or("step is not a map")?; let choose=choose_map(m.get("choose")); let br=bridges(m.get("bridges")); let input=if let Some(o)=m.get("occasion").and_then(Value::as_str){Input::RaiseOccasion{occasion:o.into(),target:m.get("target").and_then(Value::as_str).map(str::to_owned),payload:m.get("payload").and_then(Value::as_object).cloned().unwrap_or_default().into_iter().collect(),pick:pick(m.get("pick"))?,writes:writes(m.get("engine"))?}}else if let Some(by)=m.get("advance"){Input::AdvanceClock{by:advance(by)?,writes:writes(m.get("engine"))?,pick:pick(m.get("pick"))?}}else if m.contains_key("engine"){Input::HostWrite{writes:writes(m.get("engine"))?}}else if let Some(n)=m.get("event").and_then(Value::as_str){Input::WorldEvent{name:n.into()}}else if m.contains_key("newRun"){Input::NewRun{writes:if m.get("newRun").and_then(Value::as_bool)==Some(true){WritesInput::default()}else{writes(m.get("newRun"))?}}}else{return Err("unsupported step".into())}; let count=m.get("repeat").and_then(Value::as_u64).unwrap_or(1); for _ in 0..count{steps.push(ScriptStep{input:input.clone(),choose:choose.clone(),bridges:br.clone()});} }
 Ok((seed,steps,top_choose,top_bridges))
}

pub fn run_events(runtime:&Runtime, seed:Seed, steps:&[ScriptStep], top_choose:&BTreeMap<String,Vec<String>>, top_bridges:&BTreeMap<String,Vec<BTreeMap<String,Value>>>) -> Result<(String,u8),String> {
 let (mut state,mut output)=runtime.begin(seed.clone()).map_err(|e|format!("{e:?}"))?; let mut lines=vec![serde_json::to_string(&serde_json::json!({"seed":seed,"output":output})).map_err(|e|e.to_string())?]; let mut choices=BTreeMap::new(); let mut bridge_cursors=BTreeMap::new();
 for step in steps { let mut input=Some(step.input.clone()); loop { let inp=input.take().unwrap(); let result=runtime.step(state,inp.clone()); match result { Ok((s,o))=>{state=s; lines.push(serde_json::to_string(&serde_json::json!({"input":inp,"output":o})).map_err(|e|e.to_string())?); output=o;}, Err((_s,r))=>{lines.push(serde_json::to_string(&serde_json::json!({"input":inp,"rejected":r})).map_err(|e|e.to_string())?);return Ok((lines.join("\n")+"\n",1));} }
  input=match &output.await_ { Await::Choice{request,menu}=>{let q=step.choose.get(&menu.id).or_else(||top_choose.get(&menu.id)).cloned().unwrap_or_default(); let option=if menu.construct=="hub"{q.get(menu.presentation).cloned()}else if q.len()==1{q.first().cloned()}else{let c=choices.entry(menu.id.clone()).or_insert(0);let v=q.get(*c).cloned();if v.is_some(){*c+=1;}v}; Some(Input::Choose{request:*request,option:option.ok_or_else(||"incomplete: missing scripted choice".to_string())?})}, Await::Bridge{request,tag,..}=>{let q=step.bridges.get(tag).or_else(||top_bridges.get(tag)).ok_or_else(||"incomplete: missing scripted bridge".to_string())?;let c=bridge_cursors.entry(tag.clone()).or_insert(0);let fields=q.get(*c).cloned().ok_or_else(||"incomplete: bridge queue exhausted".to_string())?;*c+=1;Some(Input::BridgeResult{request:*request,fields})}, Await::Idle=>None, Await::Ended{..}|Await::Halted{..}=>None}; if input.is_none(){break;} }
  if !matches!(output.await_,Await::Idle){let code=if matches!(output.await_,Await::Ended{..}){0}else if matches!(output.await_,Await::Halted{..}){1}else{3};return Ok((lines.join("\n")+"\n",code));}
 }
 Ok((lines.join("\n")+"\n",0))
}

use serde_json::{json,Map,Value};
use std::{collections::BTreeMap,fs,path::{Path,PathBuf},process::Command,time::{SystemTime,UNIX_EPOCH}};
const BIN:&str=env!("CARGO_BIN_EXE_lute");
fn root()->PathBuf{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/edit-tasks")}
fn readj(p:&Path)->Value{serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()}
fn cp(a:&Path,b:&Path){fs::create_dir_all(b).unwrap();for e in fs::read_dir(a).unwrap(){let e=e.unwrap();let s=e.path();let d=b.join(e.file_name());if s.is_dir(){cp(&s,&d)}else{fs::copy(s,d).unwrap();}}}
fn tmp(n:&str)->PathBuf{let p=std::env::temp_dir().join(format!("lute-ed-{n}-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&p).unwrap();p}
fn cli(a:&[String])->(i32,Value){let o=Command::new(BIN).args(a).output().unwrap();let s=o.status.code().unwrap_or(2);(s,serde_json::from_slice(&o.stdout).unwrap_or_else(|e|panic!("{e}: {}",String::from_utf8_lossy(&o.stdout))))}
fn rev(p:&Path)->(String,BTreeMap<String,String>){let a=vec!["diff".into(),p.display().to_string(),p.display().to_string(),"--json".into()];let(s,v)=cli(&a);assert_eq!(s,0);let files=v["before"]["files"].as_object().unwrap().iter().filter_map(|(k,v)|v["sha256"].as_str().map(|x|(k.clone(),x.into()))).collect();(v["before"]["sha256"].as_str().unwrap().into(),files)}
fn fill(v:&mut Value,pr:&str,files:&BTreeMap<String,String>){match v{Value::String(s)=>{if s=="@BASE@"{*s=pr.into()}else if let Some(k)=s.strip_prefix("@BASE_FILE:").and_then(|x|x.strip_suffix('@')){*s=files[k].clone()}},Value::Array(a)=>a.iter_mut().for_each(|x|fill(x,pr,files)),Value::Object(o)=>o.values_mut().for_each(|x|fill(x,pr,files)),_=>{}}}
fn patch(base:&Path,v:&Value)->(i32,Value){let d=tmp("p");let f=d.join("p.json");fs::write(&f,serde_json::to_vec(v).unwrap()).unwrap();let r=cli(&vec!["patch".into(),base.display().to_string(),f.display().to_string(),"--json".into()]);let _=fs::remove_dir_all(d);r}
fn changes(v:&Value)->&[Value]{v["diff"]["changes"].as_array().map(Vec::as_slice).unwrap_or(&[])}
fn item(cs:&[Value],i:&Value)->bool{let n=i["node"].as_str().unwrap_or("");let k=i["kind"].as_str().unwrap_or("");cs.iter().any(|c|c["node"].as_str()==Some(n)&&c["kind"].as_str()==Some(k))}
fn forbidden(cs:&[Value],i:&Value)->bool{match i{Value::String(n)=>cs.iter().any(|c|c["node"].as_str()==Some(n)),Value::Object(_)=>item(cs,i),_=>false}}
fn preserved(cs:&[Value],e:&Value)->Vec<String>{let ids=e["ids"].as_array().cloned().unwrap_or_default();let mut o=vec![];for c in cs{let n=c["node"].as_str().unwrap_or("");let k=c["kind"].as_str().unwrap_or("");if ids.iter().any(|x|x.as_str()==Some(n))&&(k=="added"||k=="removed"||k.to_ascii_lowercase().contains("id")){o.push(format!("{n}:{k}"))}if e["lineIds"]==true&&k.to_ascii_lowercase().contains("lineid"){o.push(format!("{n}:{k}"))}if e["voiceKeys"]==true&&k.to_ascii_lowercase().contains("voice"){o.push(format!("{n}:{k}"))}if let Some(r)=e["reachability"].as_array(){if r.iter().any(|x|x.as_str()==Some(n))&&k=="reachability"{o.push(format!("{n}:{k}"))}}}o.sort();o.dedup();o}
fn taskdirs()->Vec<(String,PathBuf)>{let mut v:Vec<(String,PathBuf)>=vec![];for e in fs::read_dir(root()).unwrap(){let p=e.unwrap().path();if p.is_dir()&&p.file_name().unwrap().to_string_lossy().chars().next().is_some_and(|c|c.is_ascii_digit()){v.push((p.file_name().unwrap().to_string_lossy().into(),p))}}v.sort_by(|a,b|a.0.cmp(&b.0));assert_eq!(v.len(),12);v}
#[test]
fn edit_tasks_conform() {
    let mut out = Map::new();
    let mut all = true;
    for (n,t) in taskdirs() {
        let b = tmp(&n); cp(&t.join("base"), &b);
        let (pr,files) = rev(&b);
        let mut p = readj(&t.join("patch.json")); fill(&mut p,&pr,&files);
        let (s,r) = patch(&b,&p);
        let e = readj(&t.join("expect.json"));
        let valid = s != 2 && r["ok"]==true;
        let exp = e["intended"].as_array().unwrap();
        let ins = exp.iter().all(|i| item(changes(&r),i));
        let forbidden_changed = e["forbidden"].as_array().is_some_and(|xs|
            xs.iter().any(|i| forbidden(changes(&r), i)));
        let moved = exp.iter().any(|i| i["kind"]=="moved");
        let un: Vec<Value> = if valid {
            changes(&r).iter().filter(|c| c["kind"]!="vocabulary" && !(moved && c["kind"]=="moved") && !exp.iter().any(|i| item(std::slice::from_ref(c),i))).cloned().collect()
        } else { vec![json!({"error":"reference refused","report":r})] };
        let pc = if valid { preserved(changes(&r),&e["preserve"]) } else { vec!["reference refused".into()] };
        let mut tv = vec![];
        for z in fs::read_dir(&t).unwrap() {
            let q = z.unwrap().path();
            if q.file_name().unwrap().to_string_lossy().starts_with("trap-") {
                let mut x = readj(&q);
                let ex = x.as_object_mut().unwrap().remove("trapExpect").unwrap();
                fill(&mut x,&pr,&files);
                let tb=tmp("trap"); cp(&t.join("base"),&tb);
                let (st,rr)=patch(&tb,&x);
                let code_ok = ex.get("code").is_some_and(|c| rr["code"]==*c && st==2);
                let specific = if let Some(p) = ex.get("preserve").and_then(Value::as_str) {
                    let node = ex.get("node").and_then(Value::as_str).unwrap_or("");
                    rr["changes"].as_array().is_some_and(|cs| cs.iter().any(|c| c["node"]==node && c["preserve"]==p))
                } else if let Some(d) = ex.get("diagnostic").and_then(Value::as_str) {
                    rr["diagnostics"].as_array().is_some_and(|ds| ds.iter().any(|x| x["code"]==d))
                } else if let Some(n) = ex.get("node").and_then(Value::as_str) {
                    rr["changes"].as_array().is_some_and(|cs| cs.iter().any(|c| c["node"]==n))
                } else { true };
                let diff_ok = ex.get("diff").is_some_and(|i| st==1 && item(changes(&rr),i));
                let caught = specific && ((code_ok && ex.get("diff").is_none()) || (diff_ok && ex.get("code").is_none()) || (code_ok && diff_ok));
                tv.push(json!({"name":q.file_name().unwrap().to_string_lossy(),"caught":caught,"status":st,"actualCode":rr["code"],"expected":ex}));
                let _=fs::remove_dir_all(tb);
            }
        }
        let traps=tv.iter().all(|x|x["caught"]==true);
        let ok=valid&&ins&&!forbidden_changed&&un.is_empty()&&pc.is_empty()&&traps; all&=ok;
        out.insert(n,json!({"validity":valid,"intendedPresent":ins,"unintendedDiff":un,"preservedIdChanges":pc,"trapVerdicts":tv,"ok":ok}));
        let _=fs::remove_dir_all(b);
    }
    let v=Value::Object(out); let rp=root().join("REPORT.json");
    if std::env::var_os("LUTE_BLESS_EDIT_TASKS").is_some() { fs::write(rp,serde_json::to_vec_pretty(&v).unwrap()).unwrap(); }
    else { assert_eq!(readj(&rp),v,"REPORT stale"); }
    assert!(all,"edit-task conformance failed: {}",serde_json::to_string_pretty(&v).unwrap());
}

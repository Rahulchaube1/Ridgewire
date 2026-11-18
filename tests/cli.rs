use std::{io::Write,process::{Command,Stdio}};
use serde_json::Value;
fn cli(dir:&std::path::Path)->Command{
    let mut c=Command::new(env!("CARGO_BIN_EXE_ridgewire"));
    c.env("RIDGEWIRE_CONFIG_DIR",dir);
    c
}
#[test]
fn channel_state_survives_process_restart(){
    let dir=tempfile::tempdir().unwrap();
    for (cmd,enabled) in [("remove",false),("add",true)]{
        let out=cli(dir.path()).args([cmd,"github"]).output().unwrap();
        assert!(out.status.success());
        let v:Value=serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["enabled"],enabled);
        let out=cli(dir.path()).arg("doctor").output().unwrap();
        let v:Value=serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["disabled"].as_array().unwrap().is_empty(),enabled);
    }
}
#[test]
fn mixed_errors_preserve_input_order(){
    let dir=tempfile::tempdir().unwrap();
    let mut p=cli(dir.path()).args(["batch","--jobs","2"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    writeln!(p.stdin.take().unwrap(),"{{\"channel\":\"missing\",\"action\":\"one\"}}\n{{\"channel\":\"missing\",\"action\":\"two\"}}").unwrap();
    let out=p.wait_with_output().unwrap();
    assert_eq!(out.status.code(),Some(1));
    let rows:Vec<Value>=String::from_utf8(out.stdout).unwrap().lines().map(|s|serde_json::from_str(s).unwrap()).collect();
    assert_eq!(rows.len(),2);
    assert_eq!(rows[0]["action"],"one");
    assert_eq!(rows[1]["action"],"two");
}
#[test]
fn offline_route_reports_the_dispatched_action() {
    let dir = tempfile::tempdir().unwrap();
    let out = cli(dir.path()).args(["route", "https://github.com/rust-lang/rust/issues/1"]).output().unwrap();
    assert!(out.status.success());
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["channel"], "github");
    assert_eq!(result["action"], "read");
}

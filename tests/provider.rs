// Exercise the external adapter boundary without network access or a login.
#[cfg(unix)]
mod unix {
    use serde_json::Value;
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    fn search_with_fixture(body: &str) -> (std::process::ExitStatus, Value) {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("opencli");
        fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_ridgewire"))
            .env("PATH", dir.path())
            .env("RIDGEWIRE_CONFIG_DIR", dir.path())
            .args(["run", "reddit", "search", "Rust & C++", "--limit", "3"])
            .output()
            .unwrap();
        (output.status, serde_json::from_slice(&output.stdout).unwrap())
    }

    #[test]
    fn adapter_preserves_query_as_one_argument() {
        let (status, result) = search_with_fixture(r#"
            [ "$#" = 7 ] && [ "$1" = reddit ] && [ "$2" = search ] &&
            [ "$3" = 'Rust & C++' ] && [ "$4" = --limit ] &&
            [ "$5" = 3 ] && [ "$6" = -f ] && [ "$7" = json ] || exit 1
            echo '{"ok":true,"data":[{"id":42}]}'
        "#);
        assert!(status.success());
        assert_eq!(result["data"][0]["id"], 42);
    }

    #[test]
    fn provider_error_sets_exit_status() {
        let (status, result) = search_with_fixture(r#"echo '{"ok":false}'"#);
        assert_eq!(status.code(), Some(1));
        assert_eq!(result["ok"], false);
        assert!(result["error"].is_string());
    }
}

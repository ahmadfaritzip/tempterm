//! Golden and compatibility tests for P1-03 Step 2: Process Definition (def.rs).

use std::collections::BTreeMap;
use temp_terminal_procs::def::{clean_dir, host, is_valid_name, is_valid_url, Def};

#[test]
fn test_process_name_validation_go_parity() {
    // nameRE: ^[a-z0-9][a-z0-9_-]{0,31}$
    // Valid names
    for valid in [
        "a",
        "web",
        "web-server",
        "worker_1",
        "0",
        "99-bottles",
        "abcdefghijklmnopqrstuvwxyz012345", // exactly 32 chars
    ] {
        assert!(
            is_valid_name(valid),
            "expected {:?} to be a valid process name",
            valid
        );
    }

    // Invalid names
    for invalid in [
        "",                                   // empty
        "-web",                               // starts with dash
        "_web",                               // starts with underscore
        "Web",                                // uppercase
        "web app",                            // spaces
        "web.server",                         // dot
        "a234567890123456789012345678901234", // 33 chars (too long)
        "web$server",                         // symbol
    ] {
        assert!(
            !is_valid_name(invalid),
            "expected {:?} to be an invalid process name",
            invalid
        );
    }
}

#[test]
fn test_url_validation_go_parity() {
    // labelRE = ^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$
    // 1 to 3 labels, cannot end with localhost
    for ok in ["shop", "api.shop", "a-b.c.d", "x1", "a", "0"] {
        assert!(
            is_valid_url(ok).is_ok(),
            "expected valid url for {:?}, got {:?}",
            ok,
            is_valid_url(ok)
        );
    }

    for bad in [
        "",
        "Shop",
        "a..b",
        "-a",
        "a-",
        "a.b.c.d", // 4 parts > 3 parts
        "shop.localhost",
        "localhost",
        "a_b",
        "a b",
        "a.-b",
        "a.b-",
    ] {
        assert!(
            is_valid_url(bad).is_err(),
            "expected invalid url for {:?}, got Ok",
            bad
        );
    }
}

#[test]
fn test_host_function_go_parity() {
    assert_eq!(host("shop"), "shop.localhost");
    assert_eq!(host("api.shop"), "api.shop.localhost");
}

#[test]
fn test_clean_dir_go_parity() {
    assert_eq!(clean_dir(""), "");
    assert_eq!(clean_dir("."), "");
    assert_eq!(clean_dir("./"), "");
    assert_eq!(clean_dir("./web"), "web");
    assert_eq!(clean_dir("web/"), "web");
    assert_eq!(clean_dir("foo\\bar"), "foo/bar");
    assert_eq!(clean_dir("foo/./bar/"), "foo/bar");
}

#[test]
fn test_variant_golden_vectors_from_go() {
    // Vector 1: before_url_example (from Go TestVariantKeepsTrustWithoutURL)
    let def1 = Def {
        name: "web".to_string(),
        command: "npm run dev".to_string(),
        dir: "".to_string(),
        env: BTreeMap::new(),
        port: 5173,
        autostart: false,
        restart: "".to_string(),
        watch: Vec::new(),
        url: "".to_string(),
        local: false,
    };
    assert_eq!(
        def1.variant(),
        "8c1d2760524a069774a866211617cf549f3b4e3550491865938a51919e6a5528"
    );

    // Vector 2: shop_url_example
    let mut def2 = def1.clone();
    def2.url = "shop".to_string();
    assert_eq!(
        def2.variant(),
        "5193467e5f21a8ab727cbafeb7c7339fd2afebdb003cfecdf43d583cdfff9cc3"
    );

    // Vector 3: with_env_and_watch
    let mut env3 = BTreeMap::new();
    env3.insert("B".to_string(), "valB".to_string());
    env3.insert("A".to_string(), "valA".to_string());
    env3.insert("Z".to_string(), "valZ".to_string());
    let def3 = Def {
        name: "worker".to_string(),
        command: "go run ./cmd/worker".to_string(),
        dir: "./worker/".to_string(),
        env: env3,
        port: 8080,
        autostart: true, // autostart does not affect hash
        restart: "on-failure".to_string(),
        watch: vec![
            "worker/**/*.go".to_string(),
            "go.mod".to_string(),
            "config.json".to_string(),
        ],
        url: "worker.api".to_string(),
        local: true, // local does not affect hash
    };
    assert_eq!(
        def3.variant(),
        "7dd0577453c028658dfe2eaa40d13d374e6afcd33ac37839f96443f54f8edc5e"
    );

    // Vector 4: with_empty_env_and_empty_watch
    let def4 = Def {
        name: "empty_lists".to_string(),
        command: "make build".to_string(),
        dir: ".".to_string(),
        env: BTreeMap::new(),
        port: 0,
        autostart: false,
        restart: "".to_string(),
        watch: Vec::new(),
        url: "".to_string(),
        local: false,
    };
    assert_eq!(
        def4.variant(),
        "a1b163cc3eecfa64f03154939d75be3a3eda15573231e6f64ba682e8ecd3b343"
    );

    // Vector 5: clean_dir_windows_slashes
    let def5 = Def {
        name: "win_dir".to_string(),
        command: "cargo build".to_string(),
        dir: "foo\\bar".to_string(),
        env: BTreeMap::new(),
        port: 0,
        autostart: false,
        restart: "".to_string(),
        watch: Vec::new(),
        url: "".to_string(),
        local: false,
    };
    assert_eq!(
        def5.variant(),
        "3b7ffc9951a0c076a689b0afb3bdde41178f4cb5efc8406ae3cb9202f0790bbb"
    );

    // Vector 6: always_restart
    let def6 = Def {
        name: "always".to_string(),
        command: "python main.py".to_string(),
        dir: "".to_string(),
        env: BTreeMap::new(),
        port: 0,
        autostart: false,
        restart: "always".to_string(),
        watch: Vec::new(),
        url: "".to_string(),
        local: false,
    };
    assert_eq!(
        def6.variant(),
        "2f60e09b354e15456fad8ee34fe867e0198d9e4cd8e76ff519d5311e7cba69bb"
    );
}

#[test]
fn test_variant_insensitivity_to_name_autostart_and_local() {
    let base = Def {
        name: "web".to_string(),
        command: "npm run dev".to_string(),
        dir: "".to_string(),
        env: BTreeMap::new(),
        port: 3000,
        autostart: false,
        restart: "never".to_string(),
        watch: vec!["src/**".to_string()],
        url: "".to_string(),
        local: false,
    };

    let mut renamed = base.clone();
    renamed.name = "renamed-process".to_string();
    assert_eq!(renamed.variant(), base.variant());

    let mut autostart_changed = base.clone();
    autostart_changed.autostart = true;
    assert_eq!(autostart_changed.variant(), base.variant());

    let mut local_changed = base.clone();
    local_changed.local = true;
    assert_eq!(local_changed.variant(), base.variant());
}

#[test]
fn test_variant_watch_sorting_stability() {
    let def_a = Def {
        command: "test".to_string(),
        watch: vec!["b.js".to_string(), "a.js".to_string(), "c.js".to_string()],
        ..Default::default()
    };

    let def_b = Def {
        command: "test".to_string(),
        watch: vec!["a.js".to_string(), "b.js".to_string(), "c.js".to_string()],
        ..Default::default()
    };

    assert_eq!(def_a.variant(), def_b.variant());
}

#[test]
fn test_variant_restart_default_never_equivalence() {
    let def_unset = Def {
        command: "make".to_string(),
        restart: "".to_string(),
        ..Default::default()
    };

    let def_never = Def {
        command: "make".to_string(),
        restart: "never".to_string(),
        ..Default::default()
    };

    assert_eq!(def_unset.variant(), def_never.variant());

    let def_always = Def {
        command: "make".to_string(),
        restart: "always".to_string(),
        ..Default::default()
    };
    assert_ne!(def_unset.variant(), def_always.variant());
}

#[test]
fn test_validate_success_and_errors() {
    let temp_dir = std::env::temp_dir();
    let root_path = temp_dir.to_str().expect("valid root path");

    let good = Def {
        name: "web".to_string(),
        command: "npm run dev".to_string(),
        dir: "web".to_string(),
        port: 3000,
        restart: "on-failure".to_string(),
        ..Default::default()
    };
    assert!(good.validate(root_path).is_ok());

    // Bad name
    let bad_name = Def {
        name: "Web App".to_string(),
        command: "x".to_string(),
        ..Default::default()
    };
    assert!(bad_name.validate(root_path).is_err());

    // Empty command
    let empty_cmd = Def {
        name: "web".to_string(),
        command: "   ".to_string(),
        ..Default::default()
    };
    assert!(empty_cmd.validate(root_path).is_err());

    // Dir escapes project
    let dir_escapes = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        dir: "../x".to_string(),
        ..Default::default()
    };
    assert!(dir_escapes.validate(root_path).is_err());

    // Dir absolute
    let dir_abs = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        dir: "/etc".to_string(),
        ..Default::default()
    };
    assert!(dir_abs.validate(root_path).is_err());

    // Port too big
    let port_too_big = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        port: 70000,
        ..Default::default()
    };
    assert!(port_too_big.validate(root_path).is_err());

    // Port negative
    let port_neg = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        port: -1,
        ..Default::default()
    };
    assert!(port_neg.validate(root_path).is_err());

    // Invalid restart value
    let restart_bad = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        restart: "sometimes".to_string(),
        ..Default::default()
    };
    assert!(restart_bad.validate(root_path).is_err());

    // Invalid url
    let bad_url = Def {
        name: "web".to_string(),
        command: "x".to_string(),
        url: "Bad URL".to_string(),
        ..Default::default()
    };
    assert!(bad_url.validate(root_path).is_err());
}

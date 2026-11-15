use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::error::RigError;
use temp_terminal_rig::harness::expand_argv;
use temp_terminal_rig::run::{CreateRunParams, RunMode, RunRole};

#[test]
fn test_claude_argv_expansion() {
    // Headless with model, effort, session id, and prompt
    let headless = expand_argv(
        "claude",
        "headless",
        Some("opus"),
        Some("high"),
        "ses-123",
        Some("run tests"),
    )
    .unwrap();

    assert_eq!(
        headless,
        vec![
            "claude",
            "-p",
            "--dangerously-skip-permissions",
            "--session-id",
            "ses-123",
            "--model",
            "opus",
            "--effort",
            "high",
            "run tests"
        ]
    );

    // Interactive with model and effort
    let interactive = expand_argv(
        "claude",
        "interactive",
        Some("sonnet"),
        Some("medium"),
        "ses-124",
        None,
    )
    .unwrap();

    assert_eq!(
        interactive,
        vec![
            "claude",
            "--dangerously-skip-permissions",
            "--model",
            "sonnet",
            "--effort",
            "medium"
        ]
    );

    // Resume
    let resume = expand_argv(
        "claude",
        "resume",
        Some("sonnet"),
        Some("medium"),
        "ses-123",
        None,
    )
    .unwrap();

    assert_eq!(resume, vec!["claude", "--resume", "ses-123"]);
}

#[test]
fn test_codex_argv_expansion() {
    // Interactive with model and effort config
    let interactive = expand_argv(
        "codex",
        "interactive",
        Some("gpt-5"),
        Some("high"),
        "ses-456",
        None,
    )
    .unwrap();

    assert_eq!(
        interactive,
        vec![
            "codex",
            "--dangerously-bypass-approvals-and-sandbox",
            "-m",
            "gpt-5",
            "-c",
            "model_reasoning_effort=high",
        ]
    );

    // Headless with prompt
    let headless = expand_argv(
        "codex",
        "headless",
        Some("o3-mini"),
        Some("low"),
        "ses-456",
        Some("write report"),
    )
    .unwrap();

    assert_eq!(
        headless,
        vec![
            "codex",
            "exec",
            "--dangerously-bypass-approvals-and-sandbox",
            "-m",
            "o3-mini",
            "-c",
            "model_reasoning_effort=low",
            "write report",
        ]
    );

    // Resume
    let resume = expand_argv("codex", "resume", None, None, "ses-456", None).unwrap();
    assert_eq!(resume, vec!["codex", "resume", "ses-456"]);
}

#[test]
fn test_pi_and_opencode_argv_expansion() {
    // Pi headless
    let pi_headless = expand_argv("pi", "headless", None, None, "ses-pi", Some("hello")).unwrap();
    assert_eq!(
        pi_headless,
        vec!["pi", "-p", "--session-id", "ses-pi", "hello"]
    );

    // Pi interactive with model and effort
    let pi_interactive = expand_argv(
        "pi",
        "interactive",
        Some("gemini-pro"),
        Some("high"),
        "ses-pi",
        None,
    )
    .unwrap();
    assert_eq!(
        pi_interactive,
        vec![
            "pi",
            "--session",
            "ses-pi",
            "--model",
            "gemini-pro",
            "--thinking",
            "high"
        ]
    );

    // Pi resume
    let pi_resume = expand_argv("pi", "resume", None, None, "ses-pi", None).unwrap();
    assert_eq!(pi_resume, vec!["pi", "--resume", "ses-pi"]);

    // OpenCode interactive: model and effort not added in interactive
    let opencode_interactive = expand_argv(
        "opencode",
        "interactive",
        Some("gemini"),
        Some("fast"),
        "ses-oc",
        None,
    )
    .unwrap();
    assert_eq!(
        opencode_interactive,
        vec!["opencode", "--auto", "-s", "ses_ses-oc", "--prompt"]
    );

    // OpenCode headless with model and effort variant
    let opencode_headless = expand_argv(
        "opencode",
        "headless",
        Some("gemini"),
        Some("fast"),
        "ses-oc",
        Some("run build"),
    )
    .unwrap();
    assert_eq!(
        opencode_headless,
        vec![
            "opencode",
            "run",
            "--auto",
            "-s",
            "ses_ses-oc",
            "-m",
            "gemini#fast",
            "run build"
        ]
    );

    // OpenCode resume
    let opencode_resume =
        expand_argv("opencode", "resume", None, None, "ses-oc", None).unwrap();
    assert_eq!(
        opencode_resume,
        vec!["opencode", "-s", "ses_ses-oc"]
    );
}

#[test]
fn test_omp_and_commandcode_argv_expansion() {
    // OMP interactive
    let omp_interactive =
        expand_argv("omp", "interactive", Some("m1"), None, "ses-omp", None).unwrap();
    assert_eq!(omp_interactive, vec!["omp", "--auto", "-m", "m1"]);

    // OMP headless
    let omp_headless = expand_argv(
        "omp",
        "headless",
        Some("m1"),
        None,
        "ses-omp",
        Some("do work"),
    )
    .unwrap();
    assert_eq!(omp_headless, vec!["omp", "-p", "-m", "m1", "do work"]);

    // OMP resume
    let omp_resume = expand_argv("omp", "resume", None, None, "ses-omp", None).unwrap();
    assert_eq!(omp_resume, vec!["omp", "--resume", "ses-omp"]);

    // CommandCode interactive
    let cc_interactive = expand_argv(
        "commandcode",
        "interactive",
        Some("cc-model"),
        Some("max"),
        "ses-cc",
        None,
    )
    .unwrap();
    assert_eq!(
        cc_interactive,
        vec![
            "commandcode",
            "--dangerously-skip-permissions",
            "--model",
            "cc-model",
            "--effort",
            "max"
        ]
    );

    // CommandCode headless
    let cc_headless = expand_argv(
        "commandcode",
        "headless",
        Some("cc-model"),
        Some("max"),
        "ses-cc",
        Some("test suite"),
    )
    .unwrap();
    assert_eq!(
        cc_headless,
        vec![
            "commandcode",
            "-p",
            "--dangerously-skip-permissions",
            "--session-id",
            "ses-cc",
            "--model",
            "cc-model",
            "--effort",
            "max",
            "test suite"
        ]
    );

    // CommandCode resume
    let cc_resume = expand_argv("commandcode", "resume", None, None, "ses-cc", None).unwrap();
    assert_eq!(cc_resume, vec!["commandcode", "--resume", "ses-cc"]);
}

#[test]
fn test_unknown_harness_returns_error() {
    let err = expand_argv("unknown", "headless", None, None, "ses", None).unwrap_err();
    match err {
        RigError::InvalidParameter(msg) => {
            assert!(msg.contains("unknown harness: unknown"));
        }
        other => panic!("expected InvalidParameter error, got: {other:?}"),
    }

    let err_kind = expand_argv("claude", "invalid_kind", None, None, "ses", None).unwrap_err();
    match err_kind {
        RigError::InvalidParameter(msg) => {
            assert!(msg.contains("unknown launch kind: invalid_kind"));
        }
        other => panic!("expected InvalidParameter error, got: {other:?}"),
    }
}

#[test]
fn test_run_crud_and_active_run_query() {
    let db = RigDatabase::open_in_memory().unwrap();

    // Verify initial active run is None
    let active_none = db.get_active_run_for_ticket("T-1").unwrap();
    assert!(active_none.is_none());

    // Create a run for T-1
    let create_params = CreateRunParams {
        ticket: "T-1",
        role: RunRole::Worker,
        harness: "claude",
        model: Some("claude-3-7-sonnet"),
        effort: Some("high"),
        mode: RunMode::Pane,
        pane: Some("p-1"),
        sid: "ses-t1-worker",
        pid: None,
        cmd: "claude --dangerously-skip-permissions",
        status: "running",
        cwd: "/path/to/worktree",
    };
    let run = db.create_run(&create_params).unwrap();
    assert_eq!(run.id, 1);
    assert_eq!(run.ticket, "T-1");
    assert_eq!(run.role, RunRole::Worker);
    assert_eq!(run.harness, "claude");
    assert_eq!(run.model.as_deref(), Some("claude-3-7-sonnet"));
    assert_eq!(run.effort.as_deref(), Some("high"));
    assert_eq!(run.mode, RunMode::Pane);
    assert_eq!(run.pane.as_deref(), Some("p-1"));
    assert_eq!(run.sid, "ses-t1-worker");
    assert_eq!(run.pid, None);
    assert_eq!(run.cmd, "claude --dangerously-skip-permissions");
    assert_eq!(run.status, "running");
    assert_eq!(run.cwd, "/path/to/worktree");
    assert!(!run.started.is_empty());

    // Fetch run by ID
    let fetched = db.get_run(run.id).unwrap();
    assert_eq!(fetched, run);

    // Active run query should return this run
    let active = db.get_active_run_for_ticket("T-1").unwrap();
    assert!(active.is_some());
    assert_eq!(active.unwrap().id, run.id);

    // Update run status and assign pid
    db.update_run_status(run.id, "running", Some(12345))
        .unwrap();
    let updated = db.get_run(run.id).unwrap();
    assert_eq!(updated.status, "running");
    assert_eq!(updated.pid, Some(12345));

    // Update run status to done
    db.update_run_status(run.id, "done", None).unwrap();
    let done_run = db.get_run(run.id).unwrap();
    assert_eq!(done_run.status, "done");
    assert_eq!(done_run.pid, Some(12345)); // pid retained

    // Active run query should now return None
    let active_after_done = db.get_active_run_for_ticket("T-1").unwrap();
    assert!(active_after_done.is_none());

    // Create a second run for T-1 (Reviewer) with overflow mode
    let create_reviewer = CreateRunParams {
        ticket: "T-1",
        role: RunRole::Reviewer,
        harness: "codex",
        model: Some("gpt-5"),
        effort: None,
        mode: RunMode::Overflow(2),
        pane: None,
        sid: "ses-t1-reviewer",
        pid: Some(54321),
        cmd: "codex interactive",
        status: "running",
        cwd: "/path/to/repo",
    };
    let rev_run = db.create_run(&create_reviewer).unwrap();
    assert_eq!(rev_run.id, 2);
    assert_eq!(rev_run.role, RunRole::Reviewer);
    assert_eq!(rev_run.mode, RunMode::Overflow(2));

    // List runs for T-1
    let runs = db.list_runs_for_ticket("T-1").unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, run.id);
    assert_eq!(runs[1].id, rev_run.id);

    // Active run should return rev_run (which has status "running")
    let active_rev = db.get_active_run_for_ticket("T-1").unwrap();
    assert_eq!(active_rev.map(|r| r.id), Some(rev_run.id));

    // Error cases
    assert!(matches!(
        db.get_run(999).unwrap_err(),
        RigError::RunNotFound(_)
    ));
    assert!(matches!(
        db.update_run_status(999, "done", None).unwrap_err(),
        RigError::RunNotFound(_)
    ));
}

#[test]
fn test_run_mode_and_role_conversions() {
    assert_eq!(RunRole::Worker.as_str(), "worker");
    assert_eq!(RunRole::Reviewer.as_str(), "reviewer");
    assert_eq!("worker".parse::<RunRole>().unwrap(), RunRole::Worker);
    assert_eq!("reviewer".parse::<RunRole>().unwrap(), RunRole::Reviewer);
    assert!("other".parse::<RunRole>().is_err());

    assert_eq!(RunMode::Pane.to_string(), "pane");
    assert_eq!(RunMode::Tab.to_string(), "tab");
    assert_eq!(RunMode::Headless.to_string(), "headless");
    assert_eq!(RunMode::Space.to_string(), "space");
    assert_eq!(RunMode::Overflow(3).to_string(), "overflow:3");

    assert_eq!("pane".parse::<RunMode>().unwrap(), RunMode::Pane);
    assert_eq!("tab".parse::<RunMode>().unwrap(), RunMode::Tab);
    assert_eq!("headless".parse::<RunMode>().unwrap(), RunMode::Headless);
    assert_eq!("space".parse::<RunMode>().unwrap(), RunMode::Space);
    assert_eq!("overflow:3".parse::<RunMode>().unwrap(), RunMode::Overflow(3));
    assert_eq!("overflow_4".parse::<RunMode>().unwrap(), RunMode::Overflow(4));
    assert_eq!("overflow-5".parse::<RunMode>().unwrap(), RunMode::Overflow(5));
}

use temp_terminal_rig::db::RigDatabase;
use temp_terminal_rig::RunRole;
use temp_terminal_rig::supervisor::{ProcessLiveness, ProcessSupervisor};
use temp_terminal_rig::error::RigError;
use temp_terminal_rig::CreateRunParams;
use std::process::Command;

#[test]
fn test_check_pid_liveness_running_and_not_found() {
    let child = Command::new("powershell")
        .args(["-Command", "Start-Sleep -Seconds 5"])
        .spawn()
        .expect("should spawn helper process");

    let pid = child.id();
    // Give windows tasklist a brief moment to index process
    std::thread::sleep(std::time::Duration::from_millis(500));

    let liveness = ProcessSupervisor::check_pid_liveness(pid);
    match liveness {
        ProcessLiveness::Running(p) => assert_eq!(p, pid),
        other => panic!("expected Running({}), got {:?}", pid, other),
    }

    let mut child_mut = child;
    let _ = child_mut.kill();
    let _ = child_mut.wait();

    let dead_liveness = ProcessSupervisor::check_pid_liveness(999999);
    assert_eq!(dead_liveness, ProcessLiveness::NotFound);
}

#[test]
fn test_terminate_pid() {
    let child = Command::new("powershell")
        .args(["-Command", "Start-Sleep -Seconds 10"])
        .spawn()
        .expect("should spawn helper process");

    let pid = child.id();
    std::thread::sleep(std::time::Duration::from_millis(500));

    assert!(matches!(ProcessSupervisor::check_pid_liveness(pid), ProcessLiveness::Running(_)));

    ProcessSupervisor::terminate_pid(pid, true).expect("should terminate pid");

    std::thread::sleep(std::time::Duration::from_millis(500));

    let liveness = ProcessSupervisor::check_pid_liveness(pid);
    assert!(matches!(liveness, ProcessLiveness::Terminated | ProcessLiveness::NotFound | ProcessLiveness::Exited(_)));
}

#[test]
fn test_poll_active_runs_detects_exit() -> Result<(), RigError> {
    let mut db = RigDatabase::open_in_memory()?;

    db.create_run(&CreateRunParams {
        ticket: "T-1",
        role: RunRole::Worker,
        harness: "claude",
        model: None,
        effort: None,
        mode: temp_terminal_rig::run::RunMode::Pane,
        pane: None,
        sid: "sid-1",
        pid: Some(999999), // dead PID
        cmd: "some command",
        status: "running",
        cwd: ".",
    })?;

    let supervised = ProcessSupervisor::poll_active_runs(&mut db)?;
    assert_eq!(supervised.len(), 1);
    assert!(matches!(supervised[0].liveness, ProcessLiveness::NotFound | ProcessLiveness::Exited(_)));

    let updated_run = db.get_run(1)?;
    assert_eq!(updated_run.status, "exited");

    Ok(())
}

#[test]
fn test_handle_unprompted_stop() -> Result<(), RigError> {
    let mut db = RigDatabase::open_in_memory()?;

    let tail_lines = vec!["line 1".to_string(), "line 2".to_string()];
    ProcessSupervisor::handle_unprompted_stop(
        &mut db,
        42,
        "T-10",
        "worker",
        &tail_lines,
    )?;

    let inbox = db.read_inbox("manager", false, None)?;
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].too, "manager");
    assert!(inbox[0].body.contains("T-10") || inbox[0].body.contains("run #42"));

    Ok(())
}

use assert_cmd::prelude::*; // Add methods on commands
use duct::cmd;
use predicates::prelude::*; // Used for writing assertions
use std::{
    io::Error,
    io::{BufRead, BufReader, Write},
    process::Command,
};
use tempfile::NamedTempFile;

#[test]
fn usage() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("kill-orphan")?;
    cmd.assert().failure().stderr(predicate::str::contains(
        "Usage: kill-orphan <command> [<args>...]",
    ));
    Ok(())
}

#[test]
fn stdout() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("kill-orphan")?;
    cmd.env("RUST_LOG", "trace");
    cmd.arg("sh");
    cmd.arg("-c");
    cmd.arg("echo hello");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("hello"))
        .stderr(predicate::str::contains(
            r#"Launching command: ["sh", "-c", "echo hello"]"#,
        ))
        .stderr(predicate::str::contains("Spawned process with pid"))
        .stderr(predicate::str::contains(
            "Process exited with status: Some(0)",
        ));
    Ok(())
}

#[test]
fn stderr() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("kill-orphan")?;
    cmd.env("RUST_LOG", "trace");
    cmd.arg("sh");
    cmd.arg("-c");
    cmd.arg("echo hello>&2");
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("\nhello\n"))
        .stderr(predicate::str::contains(
            r#"Launching command: ["sh", "-c", "echo hello>&2"]"#,
        ))
        .stderr(predicate::str::contains("Spawned process with pid"))
        .stderr(predicate::str::contains(
            "Process exited with status: Some(0)",
        ));
    Ok(())
}

#[test]
fn exit_code() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("kill-orphan")?;
    cmd.env("RUST_LOG", "trace");
    cmd.arg("sh");
    cmd.arg("-c");
    cmd.arg("false");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            r#"Launching command: ["sh", "-c", "false"]"#,
        ))
        .stderr(predicate::str::contains("Spawned process with pid"))
        .stderr(predicate::str::contains(
            "Process exited with status: Some(1)",
        ));
    Ok(())
}

#[test]
fn kill_orphan_is_killed() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = Command::cargo_bin("kill-orphan")?;
    let kill_orphan_pid;
    {
        let reader_handle = cmd!(
            cmd.get_program(),
            "sh",
            "-c",
            "echo going to sleep && sleep 100 && echo done sleeping"
        )
        .env("RUST_LOG", "trace")
        .stderr_to_stdout()
        .reader()?;

        kill_orphan_pid = reader_handle.pids()[0];
        println!("Spawned kill-orphan with pid {}", kill_orphan_pid);

        let mut lines = BufReader::new(&reader_handle).lines();

        let mut next_line = || {
            let line = lines.next().unwrap()?;
            println!("{}", line);
            Ok::<String, Error>(line)
        };

        assert!(next_line().unwrap().ends_with(r#"Launching command: ["sh", "-c", "echo going to sleep && sleep 100 && echo done sleeping"]"#));

        let line_pid = next_line().unwrap();
        assert!(line_pid.contains(" Spawned process with pid "));
        let pid = line_pid
            .split_ascii_whitespace()
            .last()
            .unwrap()
            .parse::<u32>()?;

        Command::new("kill")
            .arg("-0")
            .arg(format!("{}", pid))
            .assert()
            .success();

        assert!(next_line().unwrap().ends_with("going to sleep"));

        Command::new("kill")
            .arg(format!("{}", kill_orphan_pid))
            .assert()
            .success();

        assert!(next_line()
            .unwrap()
            .ends_with("Received termination signal, killing process"));

        assert!(next_line()
            .unwrap()
            .contains(" Killing main child process "));

        assert!(next_line()
            .unwrap()
            .contains(" Killing descendant of child "));

        assert!(next_line()
            .unwrap()
            .ends_with("Process exited with status: None"));

        Command::new("kill")
            .arg("-0")
            .arg(format!("{}", pid))
            .assert()
            .failure();
    }

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", kill_orphan_pid))
        .assert()
        .failure();

    Ok(())
}

#[test]
fn test_parent_dies() -> Result<(), Box<dyn std::error::Error>> {
    let kill_orphan_cmd = Command::cargo_bin("kill-orphan")?;

    let mut script_file = NamedTempFile::new()?;

    write!(
        script_file,
        r#"
        echo starting background
        {} sh -c 'for i in $(seq 1 10); do echo background && sleep 1; done' &
        echo pid of background: $!
        sleep 3
        echo parent done
        "#,
        kill_orphan_cmd
            .get_program()
            .to_os_string()
            .to_string_lossy()
    )?;

    let reader_handle = cmd!("sh", script_file.path())
        .env("RUST_LOG", "trace")
        .stderr_to_stdout()
        .reader()?;

    let parent_process_pid = reader_handle.pids()[0];
    println!("Spawned parent process with pid {}", parent_process_pid);

    let mut lines = BufReader::new(&reader_handle).lines();

    let mut next_line = || {
        let line = lines.next().unwrap()?;
        println!("{}", line);
        Ok::<String, Error>(line)
    };

    assert_eq!(next_line().unwrap(), "starting background");

    let kill_orphan_pid_line = next_line().unwrap();
    assert!(kill_orphan_pid_line.contains("pid of background:"));

    let kill_orphan_pid = kill_orphan_pid_line
        .split_ascii_whitespace()
        .last()
        .unwrap()
        .parse::<u32>()?;
    assert!(next_line().unwrap().contains(r#"Launching command: ["sh", "-c", "for i in $(seq 1 10); do echo background && sleep 1; done"]"#));

    let line_pid = next_line().unwrap();
    assert!(line_pid.contains(" Spawned process with pid "));
    let pid = line_pid
        .split_ascii_whitespace()
        .last()
        .unwrap()
        .parse::<u32>()?;

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", parent_process_pid))
        .assert()
        .success();

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", kill_orphan_pid))
        .assert()
        .success();

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", pid))
        .assert()
        .success();

    assert_eq!(next_line().unwrap(), "background");

    reader_handle.kill()?;

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", parent_process_pid))
        .assert()
        .failure();

    assert!(next_line()
        .unwrap()
        .contains(" Parent process doesn't exist anymore, killing process"));

    assert!(next_line()
        .unwrap()
        .contains(" Killing main child process "));

    assert!(next_line()
        .unwrap()
        .contains(" Killing descendant of child "));

    assert!(next_line()
        .unwrap()
        .contains("Process exited with status: None"));

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", kill_orphan_pid))
        .assert()
        .failure();

    Command::new("kill")
        .arg("-0")
        .arg(format!("{}", pid))
        .assert()
        .failure();

    Ok(())
}

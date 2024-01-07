use log::debug;
use std::{
    env,
    error::Error,
    process,
    process::exit,
    process::Command,
    process::Stdio,
    sync::atomic::AtomicBool,
    sync::atomic::Ordering,
    sync::Arc,
    thread::sleep,
    time::{Duration, Instant},
};
use sysinfo::{Pid, System};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: kill-orphan <command> [<args>...]");
        exit(1);
    }

    env_logger::init();

    // Register signal handlers to intercept termination of this process
    let catched_termination_signal = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(
        signal_hook::consts::SIGTERM,
        Arc::clone(&catched_termination_signal),
    )?;
    signal_hook::flag::register(
        signal_hook::consts::SIGINT,
        Arc::clone(&catched_termination_signal),
    )?;
    signal_hook::flag::register(
        signal_hook::consts::SIGQUIT,
        Arc::clone(&catched_termination_signal),
    )?;

    debug!(
        "Launching command: {:?}",
        args.iter().skip(1).collect::<Vec<&String>>()
    );

    let mut cmd = Command::new(&args[1]);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.args(args.iter().skip(2).collect::<Vec<&String>>());

    let mut subprocess = cmd.spawn()?;

    debug!("Spawned process with pid {}", subprocess.id());

    let mut killed_subprocess_instant: Option<Instant> = None;

    let mut sys = System::new();
    sys.refresh_processes();

    let my_pid = process::id();
    let me = sys
        .process(Pid::from_u32(my_pid))
        .expect("Couldn't find my process information");
    let my_parent_pid = me.parent().expect("Couldn't find my parent PID");

    loop {
        match killed_subprocess_instant {
            Some(instant) => {
                if instant.elapsed().as_secs() > 5 {
                    debug!("Process didn't exit after 5 seconds, giving up");
                    exit(1)
                }
            }
            None => {
                // Check if the signal handler catched a termination signal for this process
                // If so, kill the child
                // At most 5s after the signal was catched, give up and exit
                if catched_termination_signal.load(Ordering::Relaxed) {
                    debug!("Received termination signal, killing process");
                    killed_subprocess_instant = Instant::now().into();
                    subprocess.kill()?;
                }

                // Check if parent is running
                // refresh_process returns false when the given PID can't be found anymore
                if !sys.refresh_process(my_parent_pid) {
                    debug!("Parent process doesn't exist anymore, killing process");
                    killed_subprocess_instant = Instant::now().into();
                    subprocess.kill()?;
                }
            }
        }

        // Monitor child process and exit if it exited
        if let Ok(Some(status)) = subprocess.try_wait() {
            debug!("Process exited with status: {:?}", status.code());
            exit(status.code().unwrap_or(1));
        }

        sleep(Duration::from_millis(100));
    }
}

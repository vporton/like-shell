use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::sync::{Arc, Mutex};
use std::{env::set_current_dir, path::Path};
use std::process::{Command, Stdio, Child};
use std::os::unix::process::CommandExt;
use std::io::{ErrorKind, Read};
use std::thread;

use fs_extra::dir::CopyOptions;
use tempdir::TempDir;

/// Create a temporary directory filled with a copy of `source_dir`.
pub fn temp_dir_from_template(source_dir: &Path) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new("test")?;
    let options = CopyOptions::new();
    fs_extra::dir::copy(source_dir, temp_dir.path(), &options)?;
    set_current_dir(temp_dir.path())?;
    Ok(temp_dir)
}

/// Child process (or process group) that runs till the object is dropped.
pub struct TermoraryChild {
    child: Child,
}

pub struct Capture {
    stdout: Option<Arc<Mutex<String>>>,
    stderr: Option<Arc<Mutex<String>>>,
}

impl TermoraryChild {
    /// Spawn child process with optional capturing its output.
    pub fn spawn(cmd: &mut Command, capture: Capture) -> std::io::Result<Self> {
        if capture.stdout.is_some() {
            cmd.stdout(Stdio::piped());
        }

        if capture.stderr.is_some() {
            cmd.stderr(Stdio::piped());
        }

        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut child = cmd.spawn()?;

        // Threads terminate, when the child exits.

        if let Some(capture_stdout) = capture.stdout {
            let stdout = child.stdout.take().unwrap();
            spawn_dump_to_string(Box::new(stdout), capture_stdout)
        }

        if let Some(capture_stderr) = capture.stderr {
            let stderr = child.stderr.take().unwrap();
            spawn_dump_to_string(Box::new(stderr), capture_stderr)
        }

        Ok(TermoraryChild {
            child,
        })
    }
}

fn spawn_dump_to_string(mut stream: Box<dyn std::io::Read + Send + Sync>, string: Arc<Mutex<String>>) {
    thread::spawn(move || {
        let mut buf = [0; 4096];
        loop {
            let r = stream.read(&mut buf);
            match r {
                Err(err) => if err.kind() == ErrorKind::UnexpectedEof {
                    return; // terminate the thread.
                } else {
                    panic!("Error in reading child output: {}", err);
                },
                Ok(r) => {
                    let s = OsString::from_vec(Vec::from(&buf[..r]));
                    string.lock().unwrap().push_str(s.to_str().expect("Wrong text encoding in child output"));
                }
            }
        }
    });
}

impl Drop for TermoraryChild {
    fn drop(&mut self) {
        // Get the process group ID of the child process
        let pid = -(self.child.id() as i32) as i32; // Negative PID targets process group

        unsafe { libc::kill(pid, libc::SIGTERM); } // Send SIGTERM to the group
    }
}


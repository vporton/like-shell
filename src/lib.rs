use std::ffi::OsString;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStringExt;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::{env::set_current_dir, path::Path};

use anyhow::bail;
use fs_extra::dir::CopyOptions;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use tempdir::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Create a temporary directory filled with a copy of `source_dir`.
pub fn temp_dir_from_template(source_dir: &Path) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new("test")?;
    let options = CopyOptions::new().content_only(true); // FIXME: What should be `copy_inside` value?
    fs_extra::dir::copy(source_dir, temp_dir.path(), &options)?;
    set_current_dir(temp_dir.path())?;
    Ok(temp_dir)
}

/// Child process (or process group) that runs till the object is dropped.
pub struct TemporaryChild {
    child: Child,
}

pub struct Capture {
    pub stdout: Option<Arc<Mutex<String>>>,
    pub stderr: Option<Arc<Mutex<String>>>,
}

impl TemporaryChild {
    /// Spawn child process with optional capturing its output.
    pub async fn spawn(cmd: &mut Command, capture: Capture) -> std::io::Result<Self> {
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
            spawn_dump_to_string(Box::pin(stdout), capture_stdout).await;
        }

        if let Some(capture_stderr) = capture.stderr {
            let stderr = child.stderr.take().unwrap();
            spawn_dump_to_string(Box::pin(stderr), capture_stderr).await;
        }

        Ok(TemporaryChild { child })
    }
}

async fn spawn_dump_to_string(
    mut stream: Pin<Box<dyn AsyncRead + Send + Sync>>, // Box<dyn std::io::Read + Send + Sync>,
    string: Arc<Mutex<String>>,
) {
    tokio::spawn(async move {
        let mut buf = [0; 4096];
        loop {
            let r = stream.read(&mut buf).await;
            match r {
                Err(err) => {
                    if err.kind() == ErrorKind::UnexpectedEof {
                        return; // terminate the thread.
                    } else {
                        panic!("Error in reading child output: {}", err);
                    }
                }
                Ok(r) => {
                    let s = OsString::from_vec(Vec::from(&buf[..r]));
                    string
                        .lock()
                        .await
                        .push_str(s.to_str().expect("Wrong text encoding in child output"));
                }
            }
        }
    });
}

impl Drop for TemporaryChild {
    fn drop(&mut self) {
        if let Some(id) = self.child.id() {
            // Get the process group ID of the child process
            let pid = -(id as i32); // Negative PID targets process group

            unsafe {
                libc::kill(pid, libc::SIGTERM);
            } // Send SIGTERM to the group

            // Wait for the child process to exit
            loop {
                match waitpid(
                    Pid::from_raw(id as i32),
                    Some(WaitPidFlag::WNOHANG),
                ) {
                    Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => break,
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

pub async fn run_successful_command(cmd: &mut Command) -> anyhow::Result<()> {
    let status = cmd.status().await?;
    if !status.success() {
        match status.code() {
            Some(code) => bail!("Command failed with exit code: {}.", code),
            None => bail!("Process terminated by a signal."),
        }
    }
    Ok(())
}

pub async fn run_failed_command(cmd: &mut Command) -> anyhow::Result<()> {
    let status = cmd.status().await?;
    if status.success() {
        bail!("Command succeeded though should have failed.");
    }
    Ok(())
}

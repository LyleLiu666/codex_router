use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

/// Run `codex login` CLI command and stream output.
///
/// This function spawns `codex login` as a subprocess and calls the provided
/// callback with each line of output. After the command completes successfully,
/// it reads the resulting auth.json from the official codex location.
pub fn run_codex_login<F>(on_output: F) -> Result<()>
where
    F: Fn(String) + Send + Sync,
{
    let mut child = Command::new("codex")
        .arg("login")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn 'codex login'. Is the Codex CLI installed?")?;

    // Read stdout in a thread
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(stdout) = stdout {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                on_output(line);
            }
        }
    }

    if let Some(stderr) = stderr {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                on_output(line);
            }
        }
    }

    let status = child.wait().context("Failed to wait for codex login")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("codex login exited with status: {}", status)
    }
}

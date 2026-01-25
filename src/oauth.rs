use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use crate::config::get_codex_home;

/// Run `codex login` CLI command and stream output.
///
/// This function spawns `codex login` as a subprocess with the correct
/// CODEX_HOME environment variable so that auth is saved to this app's
/// directory, not the official codex directory.
pub fn run_codex_login<F>(on_output: F) -> Result<()>
where
    F: Fn(String) + Send + Sync,
{
    // Get the codex_router home directory to pass to the CLI
    let codex_home = get_codex_home().context("Failed to get CODEX_HOME")?;

    let mut child = Command::new("codex")
        .arg("login")
        .env("CODEX_HOME", &codex_home)
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

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use zfswatch_core::{Error, Result};

/// Abstraction over command execution for testability
#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    /// Run a command and return (stdout, stderr, success)
    async fn run(
        &self,
        cmd: &str,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<(String, String, bool)>;
}

/// Real command runner using tokio::process::Command
#[derive(Debug, Clone, Default)]
pub struct RealCommandRunner;

impl RealCommandRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CommandRunner for RealCommandRunner {
    async fn run(
        &self,
        cmd: &str,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<(String, String, bool)> {
        let mut command = Command::new(cmd);
        command.args(args);

        if stdin_input.is_some() {
            command.stdin(Stdio::piped());
        }

        let output = if let Some(input) = stdin_input {
            let mut child = command
                .spawn()
                .map_err(|e| Error::Zfs(format!("Failed to spawn {cmd}: {e}")))?;

            if let Some(stdin) = child.stdin.take() {
                let mut stdin = stdin;
                stdin
                    .write_all(format!("{input}\n").as_bytes())
                    .await
                    .map_err(|e| Error::Zfs(format!("Failed to write stdin: {e}")))?;
                stdin.shutdown().await.ok();
            }

            child
                .wait_with_output()
                .await
                .map_err(|e| Error::Zfs(format!("Command failed: {e}")))?
        } else {
            command
                .output()
                .await
                .map_err(|e| Error::Zfs(format!("Failed to run {cmd}: {e}")))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();

        Ok((stdout, stderr, success))
    }
}

/// Mock command runner for tests
#[derive(Debug, Clone)]
pub struct MockCommandRunner {
    responses: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<(String, Vec<String>), (String, String, bool)>,
        >,
    >,
    call_log: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>, Option<String>)>>>,
}

impl Default for MockCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCommandRunner {
    pub fn new() -> Self {
        Self {
            responses: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            call_log: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Register a mock response for a specific command + args combination
    pub fn add_response(&self, cmd: &str, args: &[&str], stdout: &str, stderr: &str, success: bool) {
        let key = (cmd.to_string(), args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let mut responses = self.responses.lock().unwrap();
        responses.insert(key, (stdout.to_string(), stderr.to_string(), success));
    }

    /// Get the log of all commands that were executed
    pub fn get_calls(&self) -> Vec<(String, Vec<String>, Option<String>)> {
        self.call_log.lock().unwrap().clone()
    }

    /// Clear all registered responses and call log
    pub fn clear(&self) {
        self.responses.lock().unwrap().clear();
        self.call_log.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_real_runner_echo() {
        let runner = RealCommandRunner::new();
        let (stdout, stderr, success) = runner.run("echo", &["hello", "world"], None).await.unwrap();
        assert!(success);
        assert!(stdout.contains("hello"));
        assert!(stdout.contains("world"));
        assert_eq!(stderr, "");
    }

    #[tokio::test]
    async fn test_real_runner_failure() {
        let runner = RealCommandRunner::new();
        let (_, _, success) = runner.run("false", &[], None).await.unwrap();
        assert!(!success);
    }

    #[tokio::test]
    async fn test_mock_runner_basic() {
        let runner = MockCommandRunner::new();
        runner.add_response("echo", &["hello"], "hello output", "", true);

        let (stdout, stderr, success) = runner.run("echo", &["hello"], None).await.unwrap();
        assert_eq!(stdout, "hello output");
        assert_eq!(stderr, "");
        assert!(success);
    }

    #[tokio::test]
    async fn test_mock_runner_default_response() {
        let runner = MockCommandRunner::new();
        // No response registered, should return empty success
        let (stdout, stderr, success) = runner.run("unknown", &["cmd"], None).await.unwrap();
        assert_eq!(stdout, "");
        assert_eq!(stderr, "");
        assert!(success);
    }

    #[tokio::test]
    async fn test_mock_runner_call_log() {
        let runner = MockCommandRunner::new();
        runner.add_response("test", &["a", "b"], "out", "err", false);

        let _ = runner.run("test", &["a", "b"], Some("stdin")).await;
        let calls = runner.get_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "test");
        assert_eq!(calls[0].1, vec!["a", "b"]);
        assert_eq!(calls[0].2, Some("stdin".to_string()));
    }

    #[tokio::test]
    async fn test_mock_runner_error_response() {
        let runner = MockCommandRunner::new();
        runner.add_response("zpool", &["create"], "", "invalid vdev", false);

        let (stdout, stderr, success) = runner.run("zpool", &["create"], None).await.unwrap();
        assert!(!success);
        assert_eq!(stderr, "invalid vdev");
    }
}

#[async_trait::async_trait]
impl CommandRunner for MockCommandRunner {
    async fn run(
        &self,
        cmd: &str,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<(String, String, bool)> {
        let key = (cmd.to_string(), args.iter().map(|s| s.to_string()).collect::<Vec<_>>());

        self.call_log
            .lock()
            .unwrap()
            .push((cmd.to_string(), key.1.clone(), stdin_input.map(|s| s.to_string())));

        let responses = self.responses.lock().unwrap();
        if let Some(response) = responses.get(&key) {
            Ok(response.clone())
        } else {
            // Return empty success by default
            Ok((String::new(), String::new(), true))
        }
    }
}

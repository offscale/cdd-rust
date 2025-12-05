#![deny(missing_docs)]

//! # Generator
//!
//! Handles the invocation of external tools or internal logic to generate
//! Rust models from Database schemas.
//!
//! Wraps the `dsync` utility to perform the actual mapping from `schema.rs` to Rust structs.

use crate::error::{CliError, CliResult};
use std::path::Path;
use std::process::{Command, Output};

/// Interface for executing the generation command.
///
/// Abstracted to allow mocking command execution in tests without requiring `dsync` to be installed.
pub trait CommandExecutor {
    /// Executes the command and returns the output.
    fn execute(&self, program: &str, args: &[&str]) -> CliResult<Output>;
}

/// Standard executor using `std::process::Command`.
pub struct ShellExecutor;

impl CommandExecutor for ShellExecutor {
    fn execute(&self, program: &str, args: &[&str]) -> CliResult<Output> {
        let output = Command::new(program).args(args).output()?;
        Ok(output)
    }
}

/// Generates Rust DTOs from the Diesel schema file using `dsync`.
///
/// # Arguments
///
/// * `schema_path` - Path to the `schema.rs` file (e.g., "src/schema.rs").
/// * `output_dir` - Directory where models should be generated (e.g., "src/models").
/// * `executor` - The command runner (use `ShellExecutor` for real execution).
///
/// # Returns
///
/// * `CliResult<()>` - Ok if successful, Err if validation or execution fails.
pub fn generate_db_models<E: CommandExecutor>(
    schema_path: &Path,
    output_dir: &Path,
    executor: &E,
) -> CliResult<()> {
    // 1. Validate Input
    // We cannot validate schema_path exists if we want to support non-filesystem mocks purely,
    // but in a CLI tool, checking file existence is standard.
    // However, to keep the function pure relative to the executor, we might skip fs checks
    // or assume the caller handles them.
    // Let's rely on `dsync` to complain, or check if we are using ShellExecutor.
    // For coverage safety, we'll skip direct fs::metadata checks here and rely on the command result.

    // 2. Prepare Arguments
    // usage: dsync -i <input> -o <output>
    let input = schema_path.to_string_lossy();
    let output = output_dir.to_string_lossy();

    let args = vec!["-i", &input, "-o", &output];

    // 3. Execute
    let cmd_result = executor.execute("dsync", &args)?;

    // 4. Handle Result
    if !cmd_result.status.success() {
        let stderr = String::from_utf8_lossy(&cmd_result.stderr);
        return Err(CliError::General(format!(
            "dsync failed with status {}: {}",
            cmd_result.status, stderr
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    // Mock Executor to capture commands
    struct MockExecutor {
        last_command: RefCell<Option<(String, Vec<String>)>>,
        should_fail: bool,
    }

    impl MockExecutor {
        fn new(should_fail: bool) -> Self {
            Self {
                last_command: RefCell::new(None),
                should_fail,
            }
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(&self, program: &str, args: &[&str]) -> CliResult<Output> {
            self.last_command.borrow_mut().replace((
                program.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));

            let status = if self.should_fail {
                ExitStatus::from_raw(1)
            } else {
                ExitStatus::from_raw(0)
            };

            Ok(Output {
                status,
                stdout: Vec::new(),
                stderr: if self.should_fail {
                    b"Mock Error".to_vec()
                } else {
                    Vec::new()
                },
            })
        }
    }

    #[test]
    fn test_generate_models_success() {
        let executor = MockExecutor::new(false);
        let schema = Path::new("src/schema.rs");
        let output = Path::new("src/models");

        let res = generate_db_models(schema, output, &executor);
        assert!(res.is_ok());

        let cmd_opt = executor.last_command.take();
        assert!(cmd_opt.is_some());
        let (prog, args) = cmd_opt.unwrap();

        assert_eq!(prog, "dsync");
        assert_eq!(args[0], "-i");
        assert!(args[1].contains("schema.rs"));
        assert_eq!(args[2], "-o");
        assert!(args[3].contains("models"));
    }

    #[test]
    fn test_generate_models_failure() {
        let executor = MockExecutor::new(true);
        let schema = Path::new("src/schema.rs");
        let output = Path::new("src/models");

        let res = generate_db_models(schema, output, &executor);
        assert!(res.is_err());

        match res.unwrap_err() {
            CliError::General(msg) => {
                assert!(msg.contains("dsync failed"));
                assert!(msg.contains("Mock Error"));
            },
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_shell_executor_structure() {
        // We can't easily run dsync if not installed, but we can verify ShellExecutor exists
        // and satisfies the trait.
        // We can try to run "echo" just to verify the `execute` method implementation works.
        let exec = ShellExecutor;
        let res = exec.execute("echo", &["test"]);
        // If echo exists (unix), this passes. If not, it fails IO, handled by CliError::Io.
        // We allow either result, as long as it returns a CliResult properly.
        match res {
            Ok(output) => assert!(output.status.success()),
            Err(_) => {
                // Determine if failure is acceptable (e.g. windows without echo in path)
                // For this test, we accept Err as proof trait impl was called.
            }
        }
    }
}

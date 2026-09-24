use newviso_config::ResolvedBootstrapConfig;
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let early_report_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".newviso")
        .join("crash-reports");
    let _ = newviso_bugtrap::install(early_report_dir);
    newviso_bugtrap::set_phase("bootstrap.load");
    let bootstrap = match ResolvedBootstrapConfig::load() {
        Ok(config) => config,
        Err(error) => {
            let message = format!("bootstrap failed: {error}");
            newviso_bugtrap::report_error("bootstrap_error", &message);
            newviso_host::error("newviso.app", message);
            return ExitCode::from(1);
        }
    };

    let project_root = bootstrap
        .project_path
        .as_ref()
        .map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                bootstrap.base_dir.join(path)
            }
        })
        .unwrap_or_else(|| bootstrap.base_dir.clone());
    let _ =
        newviso_bugtrap::configure_report_dir(project_root.join(".newviso").join("crash-reports"));
    newviso_bugtrap::set_context("base_dir", bootstrap.base_dir.display().to_string());
    newviso_bugtrap::set_context("project_root", project_root.display().to_string());
    newviso_bugtrap::set_context("provider_dir", bootstrap.provider_dir.display().to_string());
    newviso_bugtrap::set_context(
        "log_path",
        project_root
            .join(".newviso")
            .join("cache")
            .join("logs")
            .join("current.ulog.ndjson")
            .display()
            .to_string(),
    );
    newviso_bugtrap::set_phase("runtime.start");

    match newviso_runtime::run(bootstrap) {
        Ok(_) => {
            newviso_bugtrap::set_phase("process.exit.success");
            ExitCode::SUCCESS
        }
        Err(_error) => {
            // Runtime errors are reported before provider teardown, while all
            // module code required for logs/backtraces is still resident.
            newviso_bugtrap::set_phase("process.exit.runtime_error");
            ExitCode::from(2)
        }
    }
}

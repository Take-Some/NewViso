use newviso_config::ResolvedBootstrapConfig;
use std::process::ExitCode;

fn main() -> ExitCode {
    let bootstrap = match ResolvedBootstrapConfig::load() {
        Ok(config) => config,
        Err(error) => {
            newviso_host::error("newviso.app", format!("bootstrap failed: {error}"));
            return ExitCode::from(1);
        }
    };

    match newviso_runtime::run(bootstrap) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            newviso_host::error("newviso.app", format!("runtime failed: {error}"));
            ExitCode::from(2)
        }
    }
}

use newviso_config::ResolvedBootstrapConfig;
use std::process::ExitCode;

fn main() -> ExitCode {
    let bootstrap = match ResolvedBootstrapConfig::load() {
        Ok(config) => config,
        Err(_) => return ExitCode::from(1),
    };

    match newviso_runtime::run(bootstrap) {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

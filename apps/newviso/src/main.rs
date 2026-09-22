use newviso_config::ResolvedBootstrapConfig;

fn main() {
    println!("NewViso 0.1.0");
    println!("lightweight modular engine host");

    let bootstrap = match ResolvedBootstrapConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("bootstrap config failed: {error}");
            std::process::exit(1);
        }
    };

    match newviso_runtime::run(bootstrap) {
        Ok(report) => {
            println!(
                "NewViso shutdown complete providers={} scene='{}'",
                report.provider_count, report.scene.title
            );
        }
        Err(error) => {
            eprintln!("NewViso runtime failed: {error}");
            std::process::exit(2);
        }
    }
}

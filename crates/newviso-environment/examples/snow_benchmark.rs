//! Reproducible CPU benchmark; assets and tuning are supplied by the caller.
use newviso_environment::snow::{SnowField,SnowFieldSpec,SnowPolicy,SnowDriftPolicy,SnowDynamicsPolicy,SnowLayerPolicy};
use serde_json::{json,Value};
use std::{env,fs,time::Instant};
fn stats(mut values:Vec<f64>)->Value {
    values.sort_by(f64::total_cmp);
    json!({"median_ms":values[values.len()/2],"p95_ms":values[(values.len()-1)*95/100],"max_ms":values[values.len()-1]})
}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let args=env::args().skip(1).collect::<Vec<_>>();
    if args.len()<2 {return Err("usage: snow_benchmark <snow-field.json> <runtime.json> [iterations] [wind]".into());}
    let spec:SnowFieldSpec=serde_json::from_slice(&fs::read(&args[0])?)?;
    let config:Value=serde_json::from_slice(&fs::read(&args[1])?)?;
    let policy:SnowPolicy=serde_json::from_value(config["variables"]["engine_cold"]["snow_policy"].clone())?;
    let drift:SnowDriftPolicy=serde_json::from_value(config["variables"]["frostline"]["snow_drift"].clone())?;
    let dynamics:SnowDynamicsPolicy=serde_json::from_value(config["variables"]["frostline"]["snow_dynamics"].clone())?;
    let iterations=args.get(2).map(|x|x.parse::<usize>()).transpose()?.unwrap_or(12).max(3);
    let wind=args.get(3).map(|x|x.parse::<f32>()).transpose()?.unwrap_or(28.0);
    let mut original=SnowField::new(spec,policy)?;
    if let Some(value)=config["variables"]["frostline"].get("snow_layers"){
        original.configure_layers(serde_json::from_value::<SnowLayerPolicy>(value.clone())?)?;
        original.take_visual_changes();
    }
    let shelter=original.wind_shelter_map(drift,&[])?;
    let mut solve=Vec::new();let mut collect=Vec::new();let mut changed=0;
    for iteration in 0..iterations+3 {
        let mut field=original.clone(); // setup and clone are outside measured time
        let start=Instant::now();
        let report=field.advance_dynamics(2.0,wind,drift,&shelter,dynamics)?;
        let solver_ms=start.elapsed().as_secs_f64()*1000.0;
        let start=Instant::now();let changes=field.take_visual_changes();
        let collect_ms=start.elapsed().as_secs_f64()*1000.0;
        if iteration>=3 {solve.push(solver_ms);collect.push(collect_ms);}
        changed=changes.0.len();
        std::hint::black_box(report);
        std::hint::black_box(changes);
    }
    println!("{}",json!({"schema":"newviso.snow.cpu_benchmark.v1",
        "profile":if cfg!(debug_assertions){"optimized-debug"}else{"release"},
        "columns":original.spec.columns,"rows":original.spec.rows,"wind":wind,
        "delta_seconds":2,"iterations":iterations,"changed_vertices":changed,
        "solver":stats(solve),"change_collection":stats(collect)}));
    Ok(())
}

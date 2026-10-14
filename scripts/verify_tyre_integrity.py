"""Native regression: intact tyres survive unobstructed driving.

Runs original vehicle handling on a separate flat Bullet surface, records every
damage transition, then confirms that an explicit wheel hit still punctures.
NEWVISO_TEST_ENGINE_EXE selects an isolated build; reports retain its SHA-256.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "NewViso"
PROJECT = ROOT / "Projects/VehicleTest"
VERIFY = Path(os.environ.get("NEWVISO_TEST_REPORT_DIR", str(ENGINE / ".newviso/verification/tyre-integrity")))

SCRIPT = r'''
import * as Game from "./main.ysc";
import {VEHICLE_PHYSICS} from "./generated/vehicle_physics.ysc";
let elapsed=0,ready=-1,id:any=null,nextSample=0,actions:any={},stats:any={},failed:string[]=[];
const encode=(v:any)=>JSON.stringify(v,(_k:string,v:any)=>typeof v==="bigint"?String(v):v);
const check=(ok:boolean,text:string)=>{if(!ok&&!failed.includes(text))failed.push(text);};
export function on_start(p:any):any {
 elapsed=0;ready=-1;id=null;nextSample=0;actions={};stats={};failed=[];
 const result:any=Game.on_start(p);
 result.commands.push({op:"physics.body.upsert",entity:900008110,body_kind:"static",
  shape:{kind:"box",half_extents:[3000,.5,3000]},position:[-4000,-.5,4000],
  rotation:[0,0,0,1],friction:.72,restitution:0,density:1,participates_in_queries:true,casts_contacts:true});
 return result;
}
export function on_frame(payload:any):any {
 elapsed+=Math.min(.05,Math.max(0,Number(payload.delta_seconds)||0));
 const runtime=payload.runtime??{},cars=runtime.vehicle_presentation??[];
 const source=cars.find((c:any)=>String(c.model_name).endsWith("/rancherxl2_hi"));
 if(ready<0){
  const result:any=Game.on_frame(payload);
  if(source&&source.parts?.length>10&&runtime.physics?.bodies?.some((b:any)=>String(b.entity)===String(source.entity))){ready=elapsed;id=source.entity;}
  else if(elapsed>45){result.commands.push({op:"console.write",message:"TYRE_INTEGRITY_RESULT "+encode({passed:false,failed:["vehicle model did not load"]})});result.exit_requested=true;}
  return result;
 }
 const commands:any[]=[],t=elapsed-ready;
 const car=(runtime.vehicles?.vehicles??[]).find((c:any)=>String(c.entity)===String(id));
 const body=(runtime.physics?.bodies??[]).find((c:any)=>String(c.entity)===String(id));
 const act=(name:string,at:number,fn:()=>void)=>{if(t>=at&&!actions[name]){actions[name]=true;fn();}};
 const input=(throttle:number,brake:number,handbrake:number,steer=0)=>commands.push({op:"vehicle.input.set",entity:id,throttle,brake,handbrake,steer});
 const reset=()=>{
  const p=VEHICLE_PHYSICS.rancherxl2;
  commands.push({op:"vehicle.upsert",entity:id,class:"automobile",repair:true,recreate_body:true,
   chassis_half_extents:[.94,.22,2.35],mass_properties:p.mass_properties,convex_hulls:p.convex_hulls,
   convex_hull_components:p.convex_hull_components,reference_handling:p.handling,
   position:[-4000,.7052,5000],rotation:[0,0,0,1],linear_velocity:[0,0,0],angular_velocity:[0,0,0]},
   {op:"vehicle.cabin.set",entity:id,engine_running:true});
 };
 act("start",0,()=>{reset();input(0,1,1);});
 act("launch",4,()=>input(1,0,0));
 act("coast",18,()=>input(0,0,0));
 act("brake",22,()=>input(0,1,0));
 act("reverse",26,()=>input(-1,0,0));
 act("reverse_coast",36,()=>input(0,0,0));
 act("reverse_brake",39,()=>input(0,1,0));
 act("forward_again",42,()=>input(1,0,0));
 act("handbrake_turn",50,()=>input(0,0,1,.35));
 act("park",53,()=>input(0,1,1));
 act("explicit_hit",59,()=>commands.push({op:"vehicle.damage.apply",entity:id,request:{
  damage_type:"bullet",component:{wheel:0},raw_damage:100}}));
 if(car&&body&&t>.1){
  const wheels=car.wheels??[];
  if(t<59){
   check(wheels.every((w:any)=>w.tire_condition==="intact"),"intact tyres survive ordinary driving without a damage source");
   stats.max_heat=Math.max(stats.max_heat??0,...wheels.map((w:any)=>w.tire_temperature??0));
   stats.max_scrub=Math.max(stats.max_scrub??0,...wheels.map((w:any)=>w.sliding_speed_mps??0));
   stats.max_speed=Math.max(stats.max_speed??0,car.speed_mps??0);
   stats.ground_samples=(stats.ground_samples??0)+Number(wheels.some((w:any)=>w.contact));
   if(!actions.first_false_damage&&wheels.some((w:any)=>w.tire_condition!=="intact")){
    actions.first_false_damage=true;stats.first_false_damage={time:t,input:car.input,wheels,body};
   }
  } else if(t>59.5) {
   stats.hit_punctured=wheels[0]?.tire_condition==="punctured";
   stats.other_tyres_intact=wheels.slice(1).every((w:any)=>w.tire_condition==="intact");
  }
  if(t>=nextSample){nextSample=t+.25;commands.push({op:"console.write",message:"TYRE_INTEGRITY_SAMPLE "+encode({
   time:t,speed:car.speed_mps,input:car.input,wheels,position:body.position,
   events:(runtime.vehicles?.events??[]).filter((e:any)=>String(e.entity)===String(id))})});}
 }
 if(t>=61){
  check(stats.max_speed>10&&stats.ground_samples>300,"the scenario drives a supported vehicle");
  check(stats.hit_punctured&&stats.other_tyres_intact,"an explicit wheel hit damages only the struck tyre");
  commands.push({op:"console.write",message:"TYRE_INTEGRITY_RESULT "+encode({passed:failed.length===0,failed,stats})});
  return {commands,exit_requested:true};
 }
 return {commands};
}
export function on_event(p:any):any{return Game.on_event(p);}
export function on_shutdown(p:any):any{return Game.on_shutdown(p);}
'''


def main():
    VERIFY.mkdir(parents=True, exist_ok=True)
    script = PROJECT / "scripts/verify_tyre_integrity.ysc"
    if script.exists() and script.read_text(encoding="utf-8") != SCRIPT:
        raise SystemExit(f"existing fixture differs: {script}")
    script.write_text(SCRIPT, encoding="utf-8")
    fixture = json.loads((PROJECT / "vehicle-fragments.project.json").read_text(encoding="utf-8"))
    fixture["project"]["name"] = "Tyre Integrity Verification"
    fixture["scripts"]["entrypoint"] = "scripts/verify_tyre_integrity.ysc"
    project = PROJECT / "tyre-integrity.project.json"
    content = json.dumps(fixture, indent=2) + "\n"
    if project.exists() and project.read_text(encoding="utf-8") != content:
        raise SystemExit(f"existing fixture differs: {project}")
    project.write_text(content, encoding="utf-8")
    env = os.environ.copy()
    env.update(NEWVISO_ASSETS_DIR=str(ROOT / "Shared/Content"),
               NEWVISO_CONTENT_DIR=str(ROOT / "Shared/Content"),
               NEWVISO_PROVIDER_DIR=str(ROOT / "pluginsRuntime"),
               NEWVISO_CODECS_DIR=str(ROOT / "pluginsRuntime/codecs"))
    sdk = ROOT / "pluginsRuntime/toolchains/vulkan-sdk"
    env.update(VULKAN_SDK=str(sdk), NEWENGINE_GLSLANG_VALIDATOR=str(sdk / "Bin/glslangValidator.exe"))
    executable = Path(env.get("NEWVISO_TEST_ENGINE_EXE", str(ENGINE / "target/release/newviso.exe")))
    binary_hash = hashlib.sha256(executable.read_bytes()).hexdigest()
    log = VERIFY / "tyre-integrity-runtime.log"
    with log.open("wb") as output:
        process = subprocess.Popen([str(executable), "--base-dir", str(ENGINE), "--project", str(project)],
                                   cwd=ENGINE, env=env, stdout=output, stderr=subprocess.STDOUT)
    start = time.monotonic()
    try:
        exit_code = process.wait(timeout=180)
        timed_out = False
    except subprocess.TimeoutExpired:
        process.terminate()
        exit_code = process.wait(timeout=15)
        timed_out = True
    verdict = None
    samples = []
    errors = []
    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
        if "runtime application failed:" in line:
            errors.append(line)
        for marker in ["TYRE_INTEGRITY_SAMPLE ", "TYRE_INTEGRITY_RESULT "]:
            if marker in line:
                value = json.JSONDecoder().raw_decode(line.split(marker, 1)[1])[0]
                if marker.endswith("SAMPLE "):
                    samples.append(value)
                else:
                    verdict = value
    summary = dict(passed=bool(verdict and verdict.get("passed") and not errors and not timed_out and exit_code == 0),
                   scenario=verdict, samples=len(samples), runtime_errors=errors, timed_out=timed_out,
                   executable=str(executable), sha256=binary_hash, exit_code=exit_code, seconds=time.monotonic() - start)
    (VERIFY / "tyre-integrity-samples.json").write_text(json.dumps(samples, indent=2) + "\n", encoding="utf-8")
    (VERIFY / "tyre-integrity-result.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary), flush=True)
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())

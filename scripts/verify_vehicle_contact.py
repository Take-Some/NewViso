"""Release/Bullet regression for combined tyre contact.

Maintained fixtures live in Projects/VehicleTest. Use
NEWVISO_TEST_ENGINE_EXE to test an isolated build before installing it.
"""
from pathlib import Path
import json
import os
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "NewViso"
PROJECT = ROOT / "Projects/VehicleTest"
VERIFY = Path(os.environ.get("NEWVISO_TEST_REPORT_DIR", str(ENGINE / ".newviso/verification/vehicle-contact")))

SCRIPT = r'''
import * as Game from "./main.ysc";
import {VEHICLE_PHYSICS} from "./generated/vehicle_physics.ysc";
let elapsed=0,ready=-1,id:any=null,nextSample=0,actions:any={},stats:any={},failed:string[]=[];
const encode=(v:any)=>JSON.stringify(v,(_k:string,v:any)=>typeof v==="bigint"?String(v):v);
const check=(ok:boolean,text:string)=>{if(!ok&&!failed.includes(text))failed.push(text);};
export function on_start(p:any):any {
 elapsed=0;ready=-1;id=null;nextSample=0;actions={};stats={};failed=[];
 return Game.on_start(p);
}
export function on_frame(payload:any):any {
 elapsed+=Math.min(.05,Math.max(0,Number(payload.delta_seconds)||0));
 const runtime=payload.runtime??{},cars=runtime.vehicle_presentation??[];
 const source=cars.find((c:any)=>String(c.model_name).endsWith("/rancherxl2_hi"));
 if(ready<0){
  const result:any=Game.on_frame(payload);
  if(source&&source.parts?.length>10&&runtime.physics?.bodies?.some((b:any)=>String(b.entity)===String(source.entity))){ready=elapsed;id=source.entity;}
  else if(elapsed>45){result.commands.push({op:"console.write",message:"TYRE_CONTACT_RESULT "+encode({passed:false,failed:["vehicle model did not load"]})});result.exit_requested=true;}
  return result;
 }
 const commands:any[]=[],t=elapsed-ready;
 const car=(runtime.vehicles?.vehicles??[]).find((c:any)=>String(c.entity)===String(id));
 const body=(runtime.physics?.bodies??[]).find((c:any)=>String(c.entity)===String(id));
 const act=(name:string,at:number,fn:()=>void)=>{if(t>=at&&!actions[name]){actions[name]=true;fn();}};
 const input=(throttle:number,brake:number,handbrake:number,steer=0)=>commands.push({op:"vehicle.input.set",entity:id,throttle,brake,handbrake,steer});
 const surface=(ice:boolean)=>commands.push({op:"vehicle.surface_policy.set",default:{class:ice?"ice":"asphalt",dry_grip:ice ? .15 : 1,wet_grip:ice ? .15 : 1,snow_grip:ice ? .15 : 1}});
 const reset=(speed:number,ice:boolean,engine:boolean,brakeBias?:number,tractionTest=false)=>{
  const p=VEHICLE_PHYSICS.rancherxl2,h:any={...p.handling};
  if(brakeBias!==undefined){delete h.m_fBrakeBiasFront;h.brake_bias_front=brakeBias;}
  // Isolate the contact's traction ceiling from reference launch-loss compensation.
  // The same modest power upgrade is applied to both roads; other stages retain
  // the original handling. With stock AWD torque, ice need not limit acceleration.
  if(tractionTest){h.m_fInitialDriveForce=.36;h.m_fLowSpeedTractionLossMult=0;}
  surface(ice);
  commands.push({op:"vehicle.upsert",entity:id,class:"automobile",repair:true,recreate_body:true,
   chassis_half_extents:[.94,.22,2.35],mass_properties:p.mass_properties,convex_hulls:p.convex_hulls,
   reference_handling:h,position:[-40,.7052,60],rotation:[0,0,0,1],linear_velocity:[0,0,-speed],angular_velocity:[0,0,0]},
   {op:"vehicle.cabin.set",entity:id,engine_running:engine});
 };
 act("dry_brake",0,()=>{reset(18,false,false);input(0,1,1);});
 act("dry_brake_end",2.5,()=>stats.dry_brake_end=car?.speed_mps??100);
 act("ice_brake",3,()=>{reset(18,true,false);input(0,1,1);});
 act("ice_brake_end",5.5,()=>stats.ice_brake_end=car?.speed_mps??0);
 act("dry_launch",6,()=>{reset(0,false,true,undefined,true);input(1,0,0);});
 act("dry_launch_end",10,()=>stats.dry_launch_end=car?.speed_mps??0);
 act("ice_launch",11,()=>{reset(0,true,true,undefined,true);input(1,0,0);});
 act("ice_launch_end",15,()=>stats.ice_launch_end=car?.speed_mps??100);
 act("handbrake",16,()=>{reset(18,false,false);input(0,0,1,.4);});
 act("burnout",19,()=>{reset(0,false,true,.95);input(1,1,0);});
 if(car&&body){
  const wheels=car.wheels??[];
  check(wheels.every((w:any)=>Number.isFinite(w.angular_velocity)&&Number.isFinite(w.longitudinal_force)&&Number.isFinite(w.lateral_force)),"contact forces and wheel spin remain finite");
  check(wheels.every((w:any)=>w.friction_utilization>=0&&w.friction_utilization<=1.00001),"combined forces stay within friction capacity");
  if(t>.2&&t<2.5)stats.brake_locked=(stats.brake_locked??0)+wheels.filter((w:any)=>w.wheel_locked).length;
  if(t>16.2&&t<18.8){
   stats.handbrake_locked=(stats.handbrake_locked??0)+wheels.slice(2).filter((w:any)=>w.wheel_locked).length;
   stats.drift_yaw=Math.max(stats.drift_yaw??0,Math.abs(body.angular_velocity[1]));
   stats.drift_slip=Math.max(stats.drift_slip??0,...wheels.map((w:any)=>w.slip_intensity));
  }
  if(t>.2&&t<19)check(wheels.every((w:any)=>w.tire_condition==="intact"),"normal contact manoeuvres do not puncture tyres");
  if(t>19.2){
   stats.burnout_heat=Math.max(stats.burnout_heat??0,...wheels.map((w:any)=>w.tire_temperature??0));
   stats.burnout_scrub=Math.max(stats.burnout_scrub??0,...wheels.map((w:any)=>w.sliding_speed_mps??0));
   stats.thermal_puncture=stats.thermal_puncture||wheels.some((w:any)=>w.tire_condition==="punctured"||w.tire_condition==="rim");
  }
  if(t>=nextSample){nextSample=t+.2;commands.push({op:"console.write",message:"TYRE_CONTACT_SAMPLE "+encode({time:t,stats,speed:car.speed_mps,body,wheels})});}
 }
 if(t>=30){
  check(stats.dry_brake_end<2,"dry road brakes stop the car");
  check(stats.ice_brake_end>stats.dry_brake_end+1,"ice retains more speed under the same brakes");
  check(stats.dry_launch_end>stats.ice_launch_end+1,"dry road provides more launch traction than ice");
  check(stats.brake_locked>0&&stats.handbrake_locked>0,"service brake and handbrake really lock wheels");
  check(stats.drift_yaw>.1&&stats.drift_slip>1,"handbrake produces yaw and sliding without a scripted yaw impulse");
  check(stats.burnout_scrub>2&&stats.burnout_heat>5,"stationary driven-wheel scrub heats the tyre");
  check(stats.thermal_puncture,"a prolonged brake-held burnout retains thermal puncture");
  commands.push({op:"console.write",message:"TYRE_CONTACT_RESULT "+encode({passed:failed.length===0,failed,stats})});
  return {commands,exit_requested:true};
 }
 return {commands};
}
export function on_event(p:any):any{return Game.on_event(p);}
export function on_shutdown(p:any):any{return Game.on_shutdown(p);}
'''


def main():
    VERIFY.mkdir(parents=True, exist_ok=True)
    script = PROJECT / "scripts/verify_tire_contact.ysc"
    # The scenario is a maintained test fixture; never replace an unrelated file.
    if script.exists() and script.read_text(encoding="utf-8") != SCRIPT:
        raise SystemExit(f"existing fixture differs: {script}")
    script.write_text(SCRIPT, encoding="utf-8")
    fixture = json.loads((PROJECT / "vehicle-fragments.project.json").read_text(encoding="utf-8"))
    fixture["project"]["name"] = "Combined Tyre Contact Verification"
    fixture["scripts"]["entrypoint"] = "scripts/verify_tire_contact.ysc"
    project = PROJECT / "tire-contact.project.json"
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
    executable = env.get("NEWVISO_TEST_ENGINE_EXE", str(ENGINE / "target/release/newviso.exe"))
    log = VERIFY / "tire-contact-runtime.log"
    with log.open("wb") as output:
        process = subprocess.Popen([executable, "--base-dir", str(ENGINE), "--project", str(project)],
                                   cwd=ENGINE, env=env, stdout=output, stderr=subprocess.STDOUT)
    start = time.monotonic()
    try:
        exit_code = process.wait(timeout=150)
        timed_out = False
    except subprocess.TimeoutExpired:
        process.terminate()
        exit_code = process.wait(timeout=15)
        timed_out = True
    verdict = None
    samples = 0
    errors = []
    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
        if "runtime application failed:" in line:
            errors.append(line)
        samples += "TYRE_CONTACT_SAMPLE " in line
        if "TYRE_CONTACT_RESULT " in line:
            verdict = json.JSONDecoder().raw_decode(line.split("TYRE_CONTACT_RESULT ", 1)[1])[0]
    summary = dict(passed=bool(verdict and verdict.get("passed") and not errors and not timed_out and exit_code == 0),
                   scenario=verdict, samples=samples, runtime_errors=errors, timed_out=timed_out,
                   executable=str(executable), exit_code=exit_code, seconds=time.monotonic() - start)
    (VERIFY / "tire-contact-result.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary), flush=True)
    return 0 if summary["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())

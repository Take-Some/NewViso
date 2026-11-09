"""Isolated Bullet/Endorphin multi-actor load probe using disposable WeaponLab fixtures."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import time

from analyze_frame_trace import normalized_trace_lines, parse_trace, percentile

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent


def write_json(path: Path, content):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(content,indent=2)+"\n",encoding="utf-8")


def make_fixture(count: int, root: Path, max_physics_steps: int | None = None, script_interval: int = 1):
    project = WORKSPACE/"Projects/WeaponLab"
    fixture = root/"fixture"
    if fixture.exists():
        shutil.rmtree(fixture)
    fixture.mkdir(parents=True)
    manifest = json.loads((project/"project.json").read_text(encoding="utf-8"))
    if not any(cap.get("provider")=="engine.physics.bullet" for cap in
               manifest["capabilities"]["required"]):
        raise RuntimeError("fixture does not pin Bullet")
    manifest["project"]["id"] = f"endorphin-scaling-{count}"
    manifest["scripts"]["entrypoint"] = "scripts/newviso/default_game.ysc"
    write_json(fixture/"project.json",manifest)
    for name in ("runtime","scene","environment"):
        relative = manifest["files"][name]
        dest = fixture/relative
        dest.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(project/relative,dest)
    runtime_path=fixture/manifest["files"]["runtime"]
    runtime=json.loads(runtime_path.read_text(encoding="utf-8"))
    original=runtime["variables"]["engine_combat"]["enemies"][0]
    enemies=[]
    ids=[]
    for i in range(count):
        actor = dict(original)
        actor_id = f"endorphin.stress.{i:03}"
        ids.append(actor_id)
        col = i%8
        row = i//8
        actor.update(id=actor_id,position=[(col-3.5)*2.0,0.05,-4-row*2.0],
                     patrol=[],weapon="weapon.pistol.military")
        actor["profile"]={**original["profile"],"max_health":100,
            "combat":None,"melee":None,"armed":False,
            "block_events":True,"combat_damage":0,"armour":0}
        enemies.append(actor)
    runtime["variables"]["engine_combat"]["enemies"]=enemies
    runtime["variables"]["engine_player"]["spawn_position"]=[0,0.05,6]
    runtime["variables"]["engine_player"]["required_streamed_mesh_colliders"]=0
    runtime["window"]={**runtime.get("window",{}),"width":960,"height":600,
                       "title":f"Endorphin scaling {count}"}
    write_json(runtime_path,runtime)

    overlay=fixture/"assets/scripts/newviso"
    overlay.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(WORKSPACE/"Shared/Content/scripts/newviso/default_game.ysc",
                    overlay/"default_game_base.ysc")
    script = """import * as Game from './default_game_base.ysc';
import { createRuntimeSnapshotDecoder } from './snapshot_delta.ysc';
const decode = createRuntimeSnapshotDecoder();
const actors = ACTOR_LIST;
let startup=0;
let physicsOnly=0;
const held = new Set<string>();
const deferred = new Set<string>();
let tick=0;
let attempts = 0;
const last = new Map<string,number>();
export function on_start(raw:any):any {
    const out:any=Game.on_start(decode(raw)); PHYSICS_STEPS_SETTING return out;
}
export function on_frame(raw:any):any {
    tick++;
    const now=Number(raw.elapsed_seconds??0);
    // Once native ragdolls are active, sample a true physics-only scene.
    // Do not decode large immutable peds/physics snapshots in the VM here;
    // the engine owns all fixed-step control and trace emission.
    if(physicsOnly && SCRIPT_INTERVAL===0) {
        if(now-physicsOnly>8) return {commands:[{op:'console.write',
            message:'ENDORPHIN_STRESS_FINISH count='+actors.length+' attempts='+attempts}],
            exit_requested:true};
        return {commands:[]};
    }
    const p = decode(raw);
    p.frame={...p.frame,platform:{...p.frame.platform,focused:false}};
    const out:any = !startup || SCRIPT_INTERVAL===0 || (tick % SCRIPT_INTERVAL)==0
        ? Game.on_frame(p) : {commands:[]};
    const records=p.runtime?.peds?.physics?.bodies??[];
    const present=new Map(records.map((entry:any)=>[entry.actor_id,entry]));
    for(const record of records) if(record.endorphin?.articulation_pending) deferred.add(record.actor_id);
    if(!startup && now>1.0 && actors.every((id:string)=>{
        const record:any=present.get(id);return record?.controlled || record?.ragdoll?.active;
    })) {
        startup=now;out.commands.push({op:'console.write',message:'ENDORPHIN_STRESS_READY count='+actors.length});
    }
    if(startup && !physicsOnly && actors.every((id:string)=>(present.get(id) as any)?.ragdoll?.active && (SCRIPT_INTERVAL!==0 || held.has(id)))) {
        physicsOnly=now;
        out.commands.push({op:'console.write',message:'ENDORPHIN_STRESS_ALL_ACTIVE count='+actors.length+' deferred='+deferred.size});
    }
    if(startup && now-startup<9) {
        for(const id of actors) {
            const record:any=present.get(id);
            if(SCRIPT_INTERVAL===0 && record?.ragdoll?.active && !held.has(id)
               && record.ragdoll.parts.every((part:any)=>part.position?.[1]<1.2)) {
                const center=record.ragdoll.parts[0].position;
                // Add a real ceiling only after this rig is below it. Keep
                // recovery blocked so the measured living workload stays resident.
                out.commands.push({op:'physics.body.upsert',entity:900080000+actors.indexOf(id),
                    body_kind:'static',position:[center[0],1.4,center[2]],
                    shape:{kind:'box',half_extents:[1.4,.06,1.4]},
                    participates_in_queries:true,casts_contacts:true});
                held.add(id);
            }
            if(record?.controlled && !record?.ragdoll?.active
               && (now-(last.get(id)??-100))>2.0) {
                out.commands.push({op:'ped.endorphin.activate',actor_id:id,
                    direction:[1,0,0],impulse:1});
                last.set(id,now);attempts++;
            }
        }
    }
    if(startup && now-startup>(SCRIPT_INTERVAL===0?15:9)
       && (!physicsOnly || SCRIPT_INTERVAL!==0)) {
        out.commands.push({op:'console.write',message:
            'ENDORPHIN_STRESS_FINISH count='+actors.length+' attempts='+attempts});
        out.exit_requested=true;
    }
    return out;
}
export function on_event(raw:any):any { return Game.on_event(raw); }
export function on_shutdown(raw:any):any { return Game.on_shutdown(raw); }
""".replace("ACTOR_LIST",json.dumps(ids)).replace("SCRIPT_INTERVAL",str(script_interval)).replace(
        "PHYSICS_STEPS_SETTING",
        ("out.commands.push({op:'physics.world.configure',max_steps_per_frame:"
         + str(max_physics_steps) + "});") if max_physics_steps else ""
    )
    (overlay/"default_game.ysc").write_text(script,encoding="utf-8")
    return fixture


def final_physical_integrity(console: str, count: int):
    result = {"valid": False, "rig_count": 0, "max_anchor_separation": 0.0,
              "minimum_part_center_y": None, "errors": []}
    try:
        marker = "pre-shutdown runtime state: "
        line = next(line for line in reversed(console.splitlines()) if marker in line)
        snapshot = json.loads(line.split(marker, 1)[1])
        actors = [body for body in snapshot["peds"]["physics"]["bodies"]
                  if body["actor_id"].startswith("endorphin.stress.")]
        result["rig_count"] = len(actors)
        if len(actors) != count:
            result["errors"].append("final actor count does not match the workload")
        positions = []
        for actor in actors:
            rig = actor.get("ragdoll") or {}
            if not rig.get("active") or not rig.get("physical_skeleton"):
                result["errors"].append(actor["actor_id"]+": articulation is not resident")
                continue
            if not actor["endorphin"]["recovery"]["support_available"]:
                result["errors"].append(actor["actor_id"]+": no measured floor support")
            for part in rig["parts"]:
                for key in ("position", "rotation", "linear_velocity", "angular_velocity"):
                    if not part.get(key) or not all(math.isfinite(v) for v in part[key]):
                        result["errors"].append(actor["actor_id"]+": invalid "+key)
                if part.get("position"):
                    positions.append(part["position"][1])
            for joint in rig["joints"]:
                gap = joint["separation"]
                if not math.isfinite(gap) or gap > 0.03:
                    result["errors"].append(actor["actor_id"]+": disconnected joint")
                if math.isfinite(gap):
                    result["max_anchor_separation"] = max(result["max_anchor_separation"], gap)
        if positions:
            result["minimum_part_center_y"] = min(positions)
            if min(positions) < -0.1:
                result["errors"].append("articulated body escaped below the fixture floor")
        result["valid"] = bool(positions) and not result["errors"]
    except (KeyError, TypeError, ValueError, StopIteration) as error:
        result["errors"].append("missing/invalid final physical evidence: "+str(error))
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--counts",default="1,8,32")
    parser.add_argument("--profile",choices=["debug","release"],default="release")
    parser.add_argument("--executable",type=Path,default=None)
    parser.add_argument("--max-frames",type=int,default=10000,
                        help="Safety cap; the elapsed measurement window still ends the run.")
    parser.add_argument("--timeout",type=int,default=95)
    parser.add_argument("--max-physics-steps",type=int,default=None)
    parser.add_argument("--script-interval",type=int,default=1)
    parser.add_argument("--tag",default="scaling-20261024")
    args=parser.parse_args()
    counts=[int(item) for item in args.counts.split(",")]
    if not counts or any(x<1 or x>128 for x in counts):parser.error("invalid actor counts")
    exe=(args.executable.resolve() if args.executable else ROOT/"target"/args.profile/"newviso.exe")
    if not exe.is_file():parser.error("missing compiled engine")
    output={}
    for count in counts:
        dest=ROOT/"cache/profiles"/f"endorphin-stress-{args.tag}-{count}"
        dest.mkdir(parents=True,exist_ok=True)
        fixture=make_fixture(count,dest,args.max_physics_steps,args.script_interval)
        env=os.environ.copy()
        env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/"Shared/Content"),
                   NEWVISO_CONTENT_DIR=str(WORKSPACE/"Shared/Content"),
                   NEWVISO_PROVIDER_DIR=str(WORKSPACE/"pluginsRuntime"),
                   NEWVISO_CODECS_DIR=str(WORKSPACE/"pluginsRuntime/codecs"),
                   NEWVISO_PERF_TRACE="1",NEWVISO_PERF_TRACE_INTERVAL="1")
        log=fixture/".newviso/cache/logs/current.ulog.ndjson"
        started=time.monotonic()
        with (dest/"native-console.log").open("wb") as fh:
            try:
                r=subprocess.run([str(exe),"--base-dir",str(ROOT),
                    "--project",str(fixture),"--max-frames",str(args.max_frames)],
                    cwd=ROOT,env=env,stdout=fh,stderr=subprocess.STDOUT,
                    timeout=args.timeout)
                exit_code=r.returncode
            except subprocess.TimeoutExpired:
                exit_code="timeout"
        console=(dest/"native-console.log").read_text("utf-8",errors="replace")
        frames=[]
        if log.is_file():
            lines=normalized_trace_lines(log.read_text("utf-8",errors="replace").splitlines())
            frames=parse_trace(lines)
        # Count ragdolls observed in native runtime traces, not configured
        # enemies; always report incomplete load separately from a full pass.
        max_active=max((int(frame.get("active_ragdolls",0)) for frame in frames),default=0)
        full=[frame for frame in frames if frame.get("active_ragdolls",0)>=count]
        active=full[30:]
        partial=[frame for frame in frames if max_active>0
                 and frame.get("active_ragdolls",0)==max_active]
        partial=partial[20:] if len(partial)>30 else partial
        for sample in active:
            if sample.get("physics_steps",0)>0 and "endorphin_ms" in sample:
                sample["endorphin_ms_per_step"]=sample["endorphin_ms"]/sample["physics_steps"]
        metric={}
        for key in ("endorphin_ms_per_step","physics_steps","total_ms","physics_ms","endorphin_ms","physics_provider_ms",
                    "scripts_ms","scene_render_wall_ms","active_ragdolls","bodies","joints"):
            values=[sample[key] for sample in active if key in sample]
            if values:
                metric[key]={"avg":round(sum(values)/len(values),4),
                    "p50":round(percentile(sorted(values),0.5),4),
                    "p95":round(percentile(sorted(values),0.95),4),
                    "max":round(max(values),4)}
        def partial_stats(key):
            v=[frame[key] for frame in partial if key in frame]
            return {"avg":round(sum(v)/len(v),4),
                "p95":round(percentile(sorted(v),0.95),4),
                "max":round(max(v),4)} if v else None
        integrity=final_physical_integrity(console,count)
        valid=(exit_code==0 and len(active)>=30 and integrity["valid"] and "ENDORPHIN_STRESS_FINISH" in console
               and "runtime application failed" not in console)
        report={"valid_measurement":valid,"physical_integrity":integrity,
                "workload":"blocked_recovery" if args.script_interval==0 else "repeated_activation",
                "sampled_physics_steps":sum(frame.get("physics_steps",0) for frame in active),
                "full_target_frames":len(full),"warmup_frames":min(30,len(full)),
                "deferred_transfer_observed":"deferred=0" not in console and "ENDORPHIN_STRESS_ALL_ACTIVE" in console,
                "executable":str(exe),
                "executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),"count":count,"max_physics_steps":args.max_physics_steps,"script_interval":args.script_interval,"exit_code":exit_code,
                "elapsed_seconds":round(time.monotonic()-started,2),
                "frames_total":len(frames),"max_observed_active":max_active,
                "full_target_achieved":max_active>=count,
                "all_active_frames":len(active),
                "partial_at_peak_frames":len(partial),
                "partial_at_peak_metrics":{key:partial_stats(key)
                    for key in ("total_ms","physics_ms","physics_provider_ms","scripts_ms","joints","physics_steps")
                    if partial_stats(key)},
                "ready_logged":"ENDORPHIN_STRESS_READY" in console,
                "finished_logged":"ENDORPHIN_STRESS_FINISH" in console,
                "bullet_provider_logged":"engine.physics.bullet" in console,
                "error_logged":"runtime application failed" in console,
                "metric":metric}
        output[str(count)]=report
        write_json(dest/"report.json",report)
        print(json.dumps(report),flush=True)
    path=ROOT/"cache/profiles"/f"endorphin-stress-{args.tag}-summary.json"
    write_json(path,output)
    if not all(report["valid_measurement"] for report in output.values()):
        raise RuntimeError("incomplete Endorphin workload; see "+str(path))


if __name__=="__main__":main()

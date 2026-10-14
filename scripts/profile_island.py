"""Reproduce Island performance on a disposable project with neutral input.

Uses the production map, project controller, Shared content and providers.
The source project and its saves are never changed. Use the same viewport and
scenario for before/after comparisons. CPU orchestration and observed intervals
are always recorded; GPU timestamps are included when enabled by the provider.
These controlled probes do not guarantee performance for every camera path.
"""
import argparse
import json
import math
import os
import shutil
import statistics
import subprocess
import uuid
from pathlib import Path

PROBE = '''
import { createRuntimeSnapshotDecoder } from "./newviso/snapshot_delta.ysc";
const decodeRuntime = createRuntimeSnapshotDecoder();
import { Events } from "./newviso/events.ysc";
import { frameGame, shutdownGame, startGame } from "./fps/game.ysc";
import { CFG } from "./fps/config.ysc";
let profileFrame=0;
function merge(result: any): any {
  return {...result, commands:[...(result?.commands??[]),...Events.drainCommands()]};
}
export function on_start(payload:any): any {
  payload=decodeRuntime(payload);
  const result=merge(startGame(payload));
  result.commands.push({op:"scene.clock.set",seconds:46800},
    {op:"scene.clock.pause",paused:true},{op:"scene.weather.force",weather:payload.project.runtime.variables.island_profile.weather});
  return result;
}
export function on_frame(payload:any): any {
  payload=decodeRuntime(payload);
  const config=payload.project.runtime.variables.island_profile;
  const walking=config.scenario==="walk";
  const dt=Math.max(0,Math.min(.05,Number(payload.delta_seconds)||0));
  payload.frame.input={state:{keys:{down:walking?[41]:[],pressed:[],released:[]},
    mouse:{down:[],pressed:walking&&profileFrame===0?[1]:[],released:[],
      delta:{x:walking?(Math.PI/30)*dt/CFG.mouseSensitivity:0,y:0},wheel:{x:0,y:0}}},
    text:[],ime_commit:[]};
  payload.frame.platform={focused:true,cursor_captured:walking};
  payload.frame.camera_navigation_enabled=walking;
  profileFrame++;
  const result=merge(frameGame(payload));
  // Synthetic capture drives the controller without trapping the operator's pointer.
  result.commands=result.commands.filter((command:any)=>command.op!=="platform.cursor.set");
  result.commands.push({op:"platform.cursor.set",captured:false});
  if(config.seconds>0&&Number(payload.elapsed_seconds)>=config.seconds)result.exit_requested=true;
  return result;
}
export function on_event(payload:any): any { Events.dispatchIncoming(payload);return {commands:Events.drainCommands()}; }
export function on_shutdown(): any { return merge(shutdownGame()); }
'''

def write_json(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,indent=2)+"\n",encoding="utf-8")

def metric(values):
    values=sorted(values)
    return {"samples":len(values),"median":statistics.median(values),
        "p95":values[max(0,math.ceil(len(values)*.95)-1)],
        "p99":values[max(0,math.ceil(len(values)*.99)-1)],"maximum":max(values)}

def summarize(log,warmup):
    import re
    stages={"application":[],"render":[],"gpu":[],"provider":[]}
    gpu_frames=set()
    device=None
    state=None
    errors=[]
    previous_frame=None
    intervals=[]
    loading_rows=[]
    playable_frame=0
    pending_render=None
    skipped_unpresented=0
    for line in log.read_text(encoding="utf-8-sig").splitlines():
        event=json.loads(line)
        message=event.get("message","")
        source=event.get("source",{}).get("name","")
        if source=="newviso.perf.render":
            pending_render={k:float(v) for k,v in re.findall(r"(\w+)=([0-9]+(?:\.[0-9]+)?)",message)}
        if source=="newviso.perf" and "scene_render_ms=" in message:
            row={k:float(v) for k,v in re.findall(r"(\w+)=([0-9]+(?:\.[0-9]+)?)",message)}
            if pending_render is None:
                skipped_unpresented+=1
                previous_frame=None
                continue
            playable_frame+=1
            stamp=float(event["timestamp_utc"].rstrip("Z"))
            if previous_frame and playable_frame>warmup and row.get("frame")==previous_frame[0]+1:
                intervals.append((stamp-previous_frame[1])*1000.0)
            previous_frame=(row.get("frame"),stamp)
            if playable_frame<=warmup and "total_ms" in row:
                loading_rows.append(row)
            if playable_frame>warmup and "total_ms" in row:
                stages["application"].append(row)
                stages["render"].append(pending_render)
            pending_render=None
        if source=="newviso.render.device":
            device=json.loads(message)
        if source=="newviso.perf.gpu" and playable_frame>warmup:
            inspector=json.loads(message)
            gpu=inspector.get("gpu_timing",{})
            frame=gpu.get("frame_index",0)
            # Delayed query readback must not admit an old loading-frame sample.
            if gpu.get("enabled") and frame>warmup and frame not in gpu_frames:
                gpu_frames.add(frame)
                numeric_gpu={key:value for key,value in gpu.items() if isinstance(value,(int,float))}
                for label,value in gpu.get("targets_ms",{}).items():
                    if isinstance(value,(int,float)):
                        numeric_gpu["target:"+label+"_ms"]=value
                stages["gpu"].append(numeric_gpu)
                stages["provider"].append(inspector.get("frame",{}))
        if message.startswith("pre-shutdown runtime state: "):
            state=json.loads(message.split(": ",1)[1])
        if event.get("level") in ("ERROR","FATAL"):
            errors.append(message)
    output={"warmup_frames":warmup,"errors":errors,"metrics":{},
        "presented_game_frames":playable_frame,"skipped_unpresented_frames":skipped_unpresented}
    if device:
        output["renderer_backend"]=device.get("backend_version")
        output["device"]=device.get("device",{})
    for name,rows in stages.items():
        if not rows:continue
        output["metrics"][name]={key:metric([row[key] for row in rows if key in row])
            for key in rows[-1] if key.endswith("_ms")}
    if intervals:
        output["metrics"]["application"]["frame_interval_ms"]=metric(intervals)
        output["median_observed_fps"]=1000.0/statistics.median(intervals)
        output["mean_observed_fps"]=1000.0/statistics.mean(intervals)
        output["frames_over_60fps_budget"]=sum(value>1000.0/60.0 for value in intervals)
        output["frames_over_120fps_budget"]=sum(value>1000.0/120.0 for value in intervals)
        output["observed_budget_samples"]=len(intervals)
    if "application" in output["metrics"]:
        median=output["metrics"]["application"]["total_ms"]["median"]
        output["median_cpu_fps_equivalent"]=1000.0/median
    if loading_rows:
        output["startup_frame_ms"]=metric([row["total_ms"] for row in loading_rows])
        output["startup_longest_frames"]=sorted(loading_rows,key=lambda row:row["total_ms"],reverse=True)[:5]
    if stages["application"]:
        output["steady_longest_frames"]=sorted(stages["application"],key=lambda row:row["total_ms"],reverse=True)[:5]
    if state:
        output["streaming"]=state["asset_streaming"]
        output["water"]=state["water"]
        output["main_view_geometry"]=state["scene"]["main_view_geometry"]
        output["camera"]=state["scene"]["camera"]
    return output,state

def main():
    parser=argparse.ArgumentParser(__doc__)
    parser.add_argument("--engine",type=Path)
    parser.add_argument("--source-project",type=Path,help="Frozen project copy for matched before/after probes")
    parser.add_argument("--label",default="release")
    parser.add_argument("--frames",type=int,default=900)
    parser.add_argument("--warmup",type=int,default=400)
    parser.add_argument("--scenario",choices=["idle","walk"],default="idle")
    parser.add_argument("--weather",choices=["CLEAR","THUNDER"],default="CLEAR")
    parser.add_argument("--seconds",type=float,default=0)
    args=parser.parse_args()
    engine_root=Path(__file__).resolve().parents[1]
    workspace=engine_root.parent
    source=args.source_project or workspace/"Projects/Island"
    project=workspace/"Tools/tmp"/("island-profile-"+args.label+"-"+uuid.uuid4().hex[:8])
    project.mkdir(parents=True)
    for directory in ("scenes","environments","config"):
        shutil.copytree(source/directory,project/directory)
    shutil.copytree(source/"scripts/fps",project/"scripts/fps")
    (project/"scripts/main.ysc").write_text(PROBE,encoding="utf-8")
    shutil.copytree(source/"assets",project/"assets")
    if (source/"content").is_dir():shutil.copytree(source/"content",project/"content")
    else:(project/"content").mkdir()
    manifest=json.loads((source/"project.json").read_text(encoding="utf-8"))
    manifest["project"]["id"]="island-profile"
    manifest["project"]["name"]="Island performance probe "+args.label
    write_json(project/"project.json",manifest)
    runtime=json.loads((source/"config/runtime.json").read_text(encoding="utf-8"))
    runtime["window"]["title"]="Island controlled performance probe"
    runtime["world_persistence"]={"save_path":None,"load_on_start":False,"save_on_shutdown":False,
        "autosave_interval_seconds":0}
    runtime["scheduling"]["native_navigation_enabled"]=False
    runtime["variables"]["island_profile"]={"scenario":args.scenario,"weather":args.weather,"seconds":args.seconds}
    write_json(project/"config/runtime.json",runtime)
    env=os.environ.copy()
    paths={"NEWVISO_ASSETS_DIR":workspace/"Shared/Content",
        "NEWVISO_CONTENT_DIR":workspace/"Shared/Content","NEWVISO_PROVIDER_DIR":workspace/"pluginsRuntime",
        "NEWVISO_CODECS_DIR":workspace/"pluginsRuntime/codecs",
        "VULKAN_SDK":workspace/"pluginsRuntime/toolchains/vulkan-sdk",
        "NEWENGINE_GLSLANG_VALIDATOR":workspace/"pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe"}
    env.update({key:str(value) for key,value in paths.items()})
    env["NEWVISO_PERF_LOG_EVERY_FRAME"]="1"
    engine=args.engine or engine_root/"target/release/newviso.exe"
    command=[str(engine),"--base-dir",str(engine_root),"--project",str(project),
        "--assets-dir",str(workspace/"Shared/Content"),"--content-dir",str(workspace/"Shared/Content"),
        "--provider-dir",str(workspace/"pluginsRuntime"),"--codecs-dir",str(workspace/"pluginsRuntime/codecs"),
        "--max-frames",str(args.frames)]
    print("PROFILE_PROJECT",project,flush=True)
    with (project/"process.log").open("w",encoding="utf-8") as out:
        run=subprocess.run(command,cwd=engine_root,env=env,stdout=out,stderr=subprocess.STDOUT,timeout=180)
    if run.returncode:raise RuntimeError("Island probe failed, exit="+str(run.returncode))
    summary,state=summarize(project/".newviso/cache/logs/current.ulog.ndjson",args.warmup)
    summary.update({"scenario":args.scenario+", 13:00 "+args.weather+", production Island at 1440x900",
        "engine":str(engine),"project":str(project),"label":args.label})
    write_json(project/"summary.json",summary)
    if state:write_json(project/"state.json",state)
    print(json.dumps(summary,ensure_ascii=True),flush=True)
    if summary["errors"]:raise AssertionError(summary["errors"])
    if summary.get("streaming",{}).get("failed",0):raise AssertionError("streaming failures")

if __name__=="__main__":main()

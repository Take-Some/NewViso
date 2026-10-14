"""Run a disposable rendered climate scene, then verify checkpoint restoration.

The engine must be built first. Original project saves are never opened.
"""
import argparse
import json
import math
import os
import subprocess
import uuid
from pathlib import Path

PROBE = r'''
import { startWorld } from "./newviso/world/default_world.ysc";
let stage = 0;
let pausedSeconds = 0;
let nightSeen = false;
let daySeen = false;
function require(value: boolean, message: string): void {
  if (!value) throw new Error("CLIMATE_SMOKE_FAILED: " + message);
}
export function on_start(payload: any): any {
  require(payload.runtime?.climate?.enabled === true, "native climate did not start");
  if (payload.project?.runtime?.variables?.climate_smoke_resume === true) {
    return {exit_requested: true, commands: [{op: "console.write", level: "info",
      target: "newviso.climate.verify", message: "CLIMATE_SMOKE_RESTORE " + JSON.stringify(payload.runtime.climate)}]};
  }
  return {commands: [...startWorld(payload), {op: "scene.weather.force", weather: "RAIN"},
    {op: "scene.timecycle.region.set", region: "URBAN", blend: 1}]};
}
export function on_frame(payload: any): any {
  const c = payload.runtime.climate;
  const t = Number(payload.elapsed_seconds);
  require(c.timecycle.channel_count >= 423, "timecycle channels missing");
  require(c.cloud_hat.index >= 0 && c.cloud_hat.index <= 20, "cloud selection missing");
  nightSeen = nightSeen || c.day_night_ratio > 0.9;
  daySeen = daySeen || c.day_night_ratio < 0.1;
  if (stage === 0 && t >= 2) {
    require(c.clock.year === 2024 && c.clock.month === 2 && c.clock.day === 29, "leap-year midnight rollover");
    require(c.wetness > 0, "rain did not wet the world");
    pausedSeconds = c.clock.day_seconds;
    stage = 1;
    return {commands: [{op: "scene.clock.pause", paused: true},
      {op: "scene.weather.transition", weather: "SNOW", duration_seconds: 1}]};
  }
  if (stage === 1 && t >= 4) {
    require(c.clock.paused && Math.abs(c.clock.day_seconds - pausedSeconds) < 0.001, "paused clock advanced");
    require(c.weather.weights.SNOW === 1, "weather stopped with the game clock");
    stage = 2;
    return {commands: [{op: "scene.clock.pause", paused: false},
      {op: "scene.timecycle.modifier.use", id: "probe", name: "underwater", strength: 0.5}]};
  }
  if (stage === 2 && t >= 5) {
    stage = 3;
    return {commands: [{op: "scene.timecycle.modifier.remove", id: "probe"},
      {op: "scene.weather.force", weather: "THUNDER"},
      {op: "scene.clock.set", seconds: 46800},
      {op: "scene.clock.rate.set", time_scale: 1}]};
  }
  if (stage === 3 && t >= 6) {
    require(nightSeen && daySeen, "night/day classifications were not both observed");
    stage = 4;
    return {exit_requested: true, commands: [{op: "scene.weather.resume"},
      {op: "console.write", level: "info", target: "newviso.climate.verify",
       message: "CLIMATE_SMOKE_OK " + JSON.stringify({nightSeen, daySeen, climate: c})}]};
  }
  return {};
}
export function on_event(): any { return {}; }
export function on_shutdown(): any { return {}; }
'''

def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')

def find_message(log, marker):
    for line in log.read_text(encoding='utf-8-sig').splitlines():
        if marker not in line: continue
        row=json.loads(line)
        for value in row.values():
            if isinstance(value, str) and value.startswith(marker):
                return json.loads(value[len(marker):])
    raise AssertionError('missing ' + marker + ' in ' + str(log))

def main():
    parser=argparse.ArgumentParser(__doc__)
    parser.add_argument('--engine', type=Path)
    args=parser.parse_args()
    engine_root=Path(__file__).resolve().parents[1]
    workspace=engine_root.parent
    engine=args.engine or engine_root/'target'/'release'/'newviso.exe'
    project=workspace/'Tools'/'tmp'/('climate-probe-'+uuid.uuid4().hex)
    (project/'assets').mkdir(parents=True);(project/'content').mkdir();(project/'scripts').mkdir()
    (project/'scripts'/'main.ysc').write_text(PROBE,encoding='utf-8')
    scene=json.loads((workspace/'Projects'/'First3D'/'scenes'/'main.scene.json').read_text(encoding='utf-8'))
    write_json(project/'scenes'/'main.scene.json',scene)
    write_json(project/'environments'/'default.json',{'schema':'newviso.environment.v1'})
    runtime={'schema':'newviso.project.runtime.v1','window':{'title':'NewViso climate verification','width':960,'height':540},
        'scheduling':{'native_navigation_enabled':False},
        'variables':{'weather_gpu_fx':True,'engine_player':{'enabled':False},'engine_world':{
            'climate':{'initial_hour':23.995,'initial_date':{'year':2024,'month':2,'day':28},'time_scale':120.0,'dynamic_weather':False}}},
        'world_persistence':{'save_path':'save/world.json','load_on_start':True,'save_on_shutdown':True,'autosave_interval_seconds':0}}
    write_json(project/'config'/'runtime.json',runtime)
    manifest={'schema':'newviso.project.v1','project':{'id':'climate-verification','name':'Climate verification','version':'0.1.0'},
        'paths':{'assets':'assets','content':'content','cache':'.newviso/cache'},
        'files':{'runtime':'config/runtime.json','environment':'environments/default.json','scene':'scenes/main.scene.json'},
        'capabilities':{'required':[],'optional':[]},
        'scripts':{'enabled':True,'provider':'engine.scripting.typescript','entrypoint':'scripts/main.ysc',
            'lifecycle':{'start':'on_start','frame':'on_frame','shutdown':'on_shutdown'}}}
    write_json(project/'project.json',manifest)
    command=[str(engine),'--base-dir',str(engine_root),'--project',str(project),'--assets-dir',str(workspace/'Shared'/'Content'),
        '--content-dir',str(workspace/'Shared'/'Content'),'--provider-dir',str(workspace/'pluginsRuntime'),
        '--codecs-dir',str(workspace/'pluginsRuntime'/'codecs'),'--max-frames','3000']
    environment=os.environ.copy()
    environment['NEWVISO_ASSETS_DIR']=str(workspace/'Shared'/'Content')
    environment['NEWVISO_CONTENT_DIR']=str(workspace/'Shared'/'Content')
    environment['NEWVISO_PROVIDER_DIR']=str(workspace/'pluginsRuntime')
    environment['NEWVISO_CODECS_DIR']=str(workspace/'pluginsRuntime'/'codecs')
    log=project/'.newviso'/'cache'/'logs'/'current.ulog.ndjson'
    print('PROBE_PROJECT',project,flush=True)
    with (project/'process-first.log').open('w',encoding='utf-8') as output:
        run=subprocess.run(command,cwd=engine_root,env=environment,stdout=output,stderr=subprocess.STDOUT,timeout=90)
    if run.returncode: raise AssertionError('engine first run failed: '+str(run.returncode))
    first=find_message(log,'CLIMATE_SMOKE_OK ')
    weather=find_message(log,'pre-shutdown runtime state: ')['scene']['weather']
    fx=weather['gpu_fx']
    assert fx['resources'] and fx['source_ready'] and fx['gpu_ready']
    assert fx['collision_map_ready'], 'rain collision map did not finish'
    layers={row['name'] for row in fx['active_world_layers'] if row['weight']>0}
    assert layers=={'THUNDER_DROP','THUNDER_MIST','THUNDER_GROUND'}, layers
    envelope=json.loads((project/'save'/'world.json').read_text(encoding='utf-8'))
    checkpoint=json.loads(envelope['payload'])['climate']['state']
    (project/'world-first.json').write_text(json.dumps(envelope,indent=2)+'\n',encoding='utf-8')
    (project/'log-first.ndjson').write_bytes(log.read_bytes())
    runtime['variables']['climate_smoke_resume']=True
    write_json(project/'config'/'runtime.json',runtime)
    with (project/'process-restart.log').open('w',encoding='utf-8') as output:
        run=subprocess.run(command,cwd=engine_root,env=environment,stdout=output,stderr=subprocess.STDOUT,timeout=90)
    if run.returncode: raise AssertionError('engine restart failed: '+str(run.returncode))
    restored=find_message(log,'CLIMATE_SMOKE_RESTORE ')
    assert restored['clock']['day_seconds']==checkpoint['clock']['day_seconds']
    assert restored['weather']['weights']==checkpoint['weather']['weights']
    assert restored['cloud_hat']['index']==checkpoint['cloud_selection']['index']
    assert restored['cloud_hat']['key']==checkpoint['cloud_selection']['key']
    # QuickJS JSON number formatting can differ by one double-precision ULP.
    assert math.isclose(restored['wetness'],checkpoint['simulation']['wetness'],
        rel_tol=0,abs_tol=math.ulp(checkpoint['simulation']['wetness']))
    assert math.isclose(restored['temperature'],checkpoint['simulation']['temperature'],rel_tol=0,abs_tol=1e-6)
    assert math.isclose(restored['wind_speed'],checkpoint['simulation']['wind_speed'],rel_tol=0,abs_tol=1e-6)
    print(json.dumps({'status':'CLIMATE_RUNTIME_OK','night_seen':first['nightSeen'],'day_seen':first['daySeen'],
        'restored_clock':restored['clock'],'restored_wetness':restored['wetness'],'restored_weather':restored['weather'],
        'timecycle_channels':restored['timecycle']['channel_count'],'project':str(project)},ensure_ascii=True),flush=True)

if __name__=='__main__':main()

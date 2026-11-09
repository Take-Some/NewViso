"""Verify project profiles offline and through the actual native script lifecycle."""
from pathlib import Path
import argparse, hashlib, json, os, re, subprocess, sys, time

ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent
CACHE=ROOT/'.newviso/cache/configuration-proof'

def save(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def environment():
    env=os.environ.copy()
    env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),
               NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
               NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),
               NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'))
    env.pop('NEWVISO_RUNTIME_PROFILE',None)
    return env

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--build-profile',choices=['debug','release'],default='release')
    parser.add_argument('--native',action='store_true')
    args=parser.parse_args()
    CACHE.mkdir(parents=True,exist_ok=True)
    tool=ROOT/'target'/args.build_profile/'newviso-project-tool.exe'
    engine=ROOT/'target'/args.build_profile/'newviso.exe'
    def run_tool(command,project=None,extra=(),success=True):
        cmd=[str(tool),command,'--base-dir',str(ROOT)]
        if project:cmd.extend(['--project',str(project)])
        proc=subprocess.run(cmd+list(extra),cwd=ROOT,env=environment(),capture_output=True,encoding='utf-8',timeout=20)
        if not success:
            assert proc.returncode!=0, f'Invalid configuration accepted: {extra}'
            return {'rejected':True,'error':proc.stderr.strip()}
        assert proc.returncode==0,proc.stderr
        return json.loads(proc.stdout)
    projects=[]
    for project in sorted((WORKSPACE/'Projects').iterdir()):
        if not (project/'project.json').is_file():continue
        result=run_tool('validate',project)
        projects.append({'project':project.name,'passed':result['passed'],'profile':result['resolution']['profile']})
    project=WORKSPACE/'Projects/MadMaxWasteland'
    profiles=[]
    for name,expected in [('balanced',8),('low_memory',4),('high_throughput',12)]:
        result=run_tool('resolve',project,['--runtime-profile',name])
        assert result['settings']['streaming']['parallel_loads']==expected
        assert result['resolution']['profile']==name
        profiles.append({'profile':name,'parallel_loads':expected,'layers':result['resolution']['layers']})
    result=run_tool('resolve',project,['--runtime-profile','low_memory','--runtime-set','streaming.parallel_loads=6'])
    assert result['settings']['streaming']['parallel_loads']==6
    assert result['resolution']['field_sources']['/streaming/parallel_loads'].startswith('command_line:')
    result=run_tool('resolve',WORKSPACE/'Projects/WeaponLab',['--runtime-layer','config/profiles/runtime.low_memory.json'])
    assert result['settings']['streaming']['parallel_loads']==4
    negative=[
        run_tool('resolve',project,['--runtime-profile','unknown-profile'],False),
        run_tool('resolve',project,['--runtime-set','streaming.paralel_loads=6'],False),
        run_tool('resolve',project,['--runtime-set','streaming.parallel_loads=0'],False)]
    schema=run_tool('schema')
    assert schema['properties']['window']['x-newviso-change-timing']=='restart'
    assert schema['properties']['streaming']['x-newviso-change-timing']=='live'
    report={'passed':True,'projects':projects,'profiles':profiles,'cli_override':True,
            'shared_layer_without_project_profile':True,'negative_cases':negative,'settings_schema':True}
    if args.native:
        fixture=CACHE/'native'
        runtime={'schema':'newviso.project.runtime.v1','window':{'title':'NewViso configuration verification','width':640,'height':480},
                 'variables':{'engine_player':{'enabled':False},'engine_vehicle':{'enabled':False},
                              'engine_world':{'climate':{'enabled':False},'sky':{'enabled':False}}}}
        save(fixture/'config/runtime.json',runtime)
        for name,settings in [('parent',{'streaming':{'parallel_loads':2}}),
                              ('smoke',{'streaming':{'parallel_loads':3}}),
                              ('cli',{'streaming':{'parallel_loads':4}})]:
            save(fixture/'config'/f'{name}.json',{'schema':'newviso.project.runtime.v1',**settings})
        save(fixture/'project.json',{
            'schema':'newviso.project.v1','project':{'id':'configuration-verification','name':'Configuration verification'},
            'files':{'runtime':'config/runtime.json','environment':'environment.json','scene':'scene.json'},
            'configuration':{'profiles':{'parent':{'layers':['config/parent.json']},
                                         'smoke':{'inherits':['parent'],'layers':['config/smoke.json']}}},
            'scripts':{'enabled':True,'provider':'engine.scripting.typescript','entrypoint':'scripts/main.ysc','runtime_delta':False}})
        save(fixture/'environment.json',{'schema':'newviso.environment.v1','clear_color':[.08,.10,.12,1],
                                        'sky':None,'weather':{'enabled':False},'atmospheric_clouds':{'enabled':False}})
        save(fixture/'scene.json',{'schema':'newviso.scene.v1','version':1,'title':'Configuration verification','entities':[
            {'kind':'camera','name':'Camera','transform':{'position':[0,2,6],'target':[0,0,0],'up':[0,1,0]},
             'camera':{'projection':'perspective','fov_y_degrees':60,'near':.05,'far':500}},
            {'kind':'mesh','name':'Reference','transform':{'position':[0,0,0]},
             'mesh':{'primitive':'cube'},'material':{'base_color':[.3,.5,.7,1]},'collider':{'solid':False}}]})
        source="""import { RuntimeConfiguration } from './newviso/runtime.ysc';
let frames=0;
function check(ok:boolean,message:string):void {if(!ok)throw new Error('CONFIGURATION_VERIFY: '+message);}
export function on_start(p:any):any {
 const config=new RuntimeConfiguration(p);
 check(config.launchConfiguration?.profile==='smoke','selected profile');
 check(config.settings.streaming.parallel_loads===5,'CLI precedence');
 check(config.launchSourceOf('streaming.parallel_loads')?.startsWith('command_line:')===true,'override source');
 check(p.project.runtime.window.width===800,'cold launch setting');
 return {commands:[config.configure({streaming:{max_loads_per_tick:3},variables:{configuration_proof:{live:true}}})]};
}
export function on_frame(p:any):any {
 frames++;
 const config=new RuntimeConfiguration(p);
 check(config.settings.streaming.parallel_loads===5,'launch policy retained');
 check(config.settings.streaming.max_loads_per_tick===3,'live policy applied');
 check(p.project.runtime.variables.configuration_proof.live===true,'current project settings');
 if(frames===3)return {commands:[{op:'console.write',message:'CONFIGURATION_VERIFY_PASS '+JSON.stringify({profile:config.launchConfiguration.profile,parallel_loads:config.settings.streaming.parallel_loads,live_patch:true,source:config.launchSourceOf('streaming.parallel_loads')})}]};
 return {commands:[],exit_requested:frames>=4};
}
export function on_event(p:any):any{return {commands:[]};}
export function on_shutdown(p:any):any{return {commands:[]};}
"""
        (fixture/'scripts').mkdir(parents=True,exist_ok=True)
        (fixture/'scripts/main.ysc').write_text(source,encoding='utf-8')
        cmd=[str(engine),'--base-dir',str(ROOT),'--project',str(fixture),'--runtime-profile','smoke',
             '--runtime-layer','config/cli.json','--runtime-set','streaming.parallel_loads=5',
             '--runtime-set','window.width=800','--max-frames','240']
        with (CACHE/'native.log').open('wb') as log:
            proc=subprocess.Popen(cmd,cwd=ROOT,env=environment(),stdout=log,stderr=subprocess.STDOUT)
        (CACHE/'native.pid').write_text(str(proc.pid))
        try:code=proc.wait(timeout=150)
        except subprocess.TimeoutExpired:
            proc.terminate();proc.wait(timeout=10);raise RuntimeError('Native configuration verification timed out')
        log=(CACHE/'native.log').read_text(encoding='utf-8',errors='replace')
        errors=[line for line in log.splitlines() if any(x in line for x in ['runtime application failed:','TYPESCRIPT_SCRIPTING_COMPILE_FAILED','CONFIGURATION_VERIFY:','panicked at'])]
        assert code==0 and not errors and 'CONFIGURATION_VERIFY_PASS ' in log, {'exit_code':code,'errors':errors,'tail':log[-2500:]}
        frame_match=re.search(r'platform stopped frames=(\d+)',log)
        report['native']={'passed':True,'exit_code':code,'profile':'smoke','parallel_loads':5,'live_patch':True,'frames':int(frame_match.group(1)) if frame_match else None}
    baseline=ROOT/'.newviso/cache/engine-configuration-gameplay-baseline.json'
    if baseline.exists():
        hashes=json.loads(baseline.read_text(encoding='utf-8'))
        assert all(hashlib.sha256((WORKSPACE/p).read_bytes()).hexdigest()==sha for p,sha in hashes.items())
        report['unchanged_gameplay_modules']=len(hashes)
    save(CACHE/'verification.json',report)
    print(json.dumps(report,ensure_ascii=False))
    return 0

if __name__=='__main__':
    try:sys.exit(main())
    except Exception as error:
        save(CACHE/'verification.json',{'passed':False,'error':str(error)})
        raise

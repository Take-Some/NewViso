"""Native skeletal strike and cold-weapon integration proof using the Shared controller in disposable WeaponLab content."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time
ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent

def save(p,value):
    p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--profile',default='release',choices=['debug','release']);parser.add_argument('--scenario',default='unarmed',choices=['unarmed','weapons','director']);args=parser.parse_args()
    cache=ROOT/'.newviso/cache'/('melee-proof' if args.scenario=='unarmed' else 'melee-'+args.scenario+'-proof');fixture=cache/'fixture'
    if fixture.exists():shutil.rmtree(fixture)
    imports=json.loads((WORKSPACE/'Shared/Source/models/characters/gta/combat-peds.import.json').read_text(encoding='utf-8'))
    for character in imports['characters']:
        for resource in character['runtime_files'].values():
            data=(WORKSPACE/'Shared/Content'/resource['path']).read_bytes()
            assert hashlib.sha256(data).hexdigest()==resource['sha256'], 'changed imported resource '+resource['path']
    source=WORKSPACE/'Projects/WeaponLab'
    fixture.mkdir(parents=True)
    manifest=json.loads((source/'project.json').read_text(encoding='utf-8'))
    for key in ['runtime','environment','scene']:
        rel=manifest['files'][key];dest=fixture/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/rel,dest)
    manifest['project']['id']='melee-verification';manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
    save(fixture/'project.json',manifest)
    runtime=json.loads((fixture/manifest['files']['runtime']).read_text(encoding='utf-8'))
    authored=json.loads((WORKSPACE/'Projects/WeaponLab/config/runtime.json').read_text(encoding='utf-8'))['variables']['engine_combat']
    enemy=authored['enemies'][1 if args.scenario=='weapons' else 0].copy();enemy.update(id='melee.test.enemy',position=[0,.05,-5],patrol=[])
    enemy.pop('weapon',None)
    enemy['profile']={**enemy['profile'],'combat':None,'armed':False,'melee':{}}
    enemies=[enemy]
    if args.scenario=='director':
        enemies=[]
        for index in range(3):
            actor=authored['enemies'][index%len(authored['enemies'])].copy()
            actor.update(id='director.enemy.'+str(index),position=[index*2,.05,-5],patrol=[])
            actor.pop('weapon',None)
            actor['profile']={**actor['profile'],'combat':None,'armed':False,'melee':{}}
            enemies.append(actor)
    runtime['variables']['engine_combat']={**authored,'enemies':enemies}
    runtime['variables']['engine_player']['spawn_position']=[0,.05,6]
    runtime['variables']['engine_player']['required_streamed_mesh_colliders']=0
    runtime['window']={**runtime.get('window',{}),'title':'NorthStar fist verification','width':960,'height':600}
    save(fixture/manifest['files']['runtime'],runtime)
    overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True)
    shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
    shutil.copyfile(ROOT/('scripts/fixtures/melee_verify.ysc' if args.scenario=='unarmed' else 'scripts/fixtures/melee_weapons_verify.ysc' if args.scenario=='weapons' else 'scripts/fixtures/combat_director_verify.ysc'),overlay/'default_game.ysc')
    env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
        NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),
        VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
    start=time.time()
    command=[str(ROOT/'target'/args.profile/'newviso.exe'),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','5000']
    with (cache/'native.log').open('wb') as log:
        proc=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
        (cache/'native.pid').write_text(str(proc.pid))
        try:code=proc.wait(timeout=200)
        except subprocess.TimeoutExpired:proc.terminate();proc.wait(timeout=10);raise RuntimeError('combat native proof timed out')
    log=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
    passed=code==0 and 'MELEE_VERIFY_PASS' in log and not any(t in log for t in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
    report={'passed':passed,'exit_code':code,'seconds':round(time.time()-start,2),'male_models':len(imports['characters']),
        'checks':['shared_input','holster','original_skeleton_fists','windup','single_contact','alternating_hands','range_miss','cover','hit_stun','frontal_guard','stamina','rear_hit','male_ai','no_firearm_ammo','death_no_ghost_hit','ragdoll','weapon_return']}
    report['scenario']=args.scenario
    report['executable_sha256']=hashlib.sha256((ROOT/'target'/args.profile/'newviso.exe').read_bytes()).hexdigest()
    report['physics_provider_sha256']=hashlib.sha256((WORKSPACE/'pluginsRuntime/bullet-physics-0.3.0-release.dll').read_bytes()).hexdigest()
    for line in log.splitlines():
        if 'MELEE_VERIFY_EVIDENCE ' in line:report['evidence']=json.loads(line.split('MELEE_VERIFY_EVIDENCE ',1)[1])
    if args.scenario=='weapons':report['checks']=['shared_knife_input','real_attachment_blade_sweeps','cut_and_puncture','skeletal_receiver','world_obstruction','weapon_and_empty_hand_generations','npc_cold_weapon','physical_release','no_death_duplicate','no_ammunition']
    if args.scenario=='director':report['checks']=['authored_attack_slots','real_melee_fairness','death_reassignment','observed_guard_counter','resident_geometry_cover','reload_cover_persistence','peek_reacquisition','hidden_target_memory','memory_expiry']
    save(cache/'acceptance.json',report)
    if not passed:raise RuntimeError(log[-10000:])
    print(json.dumps(report))
if __name__=='__main__':main()

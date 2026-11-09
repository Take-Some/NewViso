"""Native integration proof for opt-in combat; all test content is disposable."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time
ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent

def save(p,value):
    p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--profile',default='release',choices=['debug','release'])
    parser.add_argument('--scenario',default='death',choices=['death','arm','heavy','critical','ballistic','adaptive','action','rearm','data_override']);args=parser.parse_args()
    cache=ROOT/'.newviso/cache'/('combat-loot-proof' if args.scenario=='death' else 'combat-disarm-'+args.scenario+'-proof');fixture=cache/'fixture'
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
    manifest['project']['id']='combat-loot-verification';manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
    save(fixture/'project.json',manifest)
    runtime=json.loads((fixture/manifest['files']['runtime']).read_text(encoding='utf-8'))
    authored=json.loads((WORKSPACE/'Projects/WeaponLab/config/runtime.json').read_text(encoding='utf-8'))['variables']['engine_combat']
    enemy=authored['enemies'][0].copy();enemy.update(id='combat.loot.enemy',position=[0,.05,-5],patrol=[])
    enemy['weapon']='weapon.rifle.mini14' if args.scenario in ['adaptive','data_override'] else 'weapon.pistol.military'
    enemy['profile']={**enemy['profile'],'max_health':100,'armour':0 if args.scenario=='death' else 30,
        'melee':enemy['profile'].get('melee') if args.scenario=='arm' else None,'armed':True,'combat_cooldown_seconds':.4,'combat_damage':10 if args.scenario=='death' else 0,
        'combat':{**enemy['profile']['combat'],'melee_only':False,'magazine_capacity':2,'reserve':4,'reaction_seconds':.1,'reload_seconds':.7,'spread':0}}
    enemies=[enemy]
    if args.scenario in ['adaptive','data_override']:
        import copy
        enemy['position']=[-3,.05,-5];enemy['profile']['armour']=0
        enemy['profile']['combat']['reaction_seconds']=10
        control=copy.deepcopy(enemy);control['id']=enemy['id']+'.control'
        control['position']=[3,.05,-5];control['weapon']='weapon.pistol.military';enemies.append(control)
    runtime['variables']['engine_combat']={**authored,'enemies':enemies}
    runtime['variables']['loot_verification_scenario']=args.scenario
    if args.scenario=='data_override':
        model=json.loads((WORKSPACE/'Shared/Content/physics/peds/humanoid_grip.json').read_text(encoding='utf-8'))
        model['hand_force']=[v*100 for v in model['hand_force']]
        model['wrist_torque']=[v*100 for v in model['wrist_torque']]
        save(fixture/'assets/physics/peds/humanoid_grip.json',model)
        runtime['variables']['grip_override_force']=model['hand_force'][0]
    runtime['variables']['engine_player']['spawn_position']=[0,.05,6]
    runtime['variables']['engine_player']['required_streamed_mesh_colliders']=0
    runtime['window']={**runtime.get('window',{}),'title':'NorthStar combat loot verification','width':960,'height':600}
    save(fixture/manifest['files']['runtime'],runtime)
    overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True)
    shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
    shutil.copyfile(ROOT/'scripts/fixtures/combat_loot_verify.ysc',overlay/'default_game.ysc')
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
    passed=code==0 and 'COMBAT_LOOT_VERIFY_PASS' in log and not any(t in log for t in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
    report={'passed':passed,'exit_code':code,'seconds':round(time.time()-start,2),'male_models':len(imports['characters']),'scenario':args.scenario,
        'checks':['native_atomic_transfer','socket_pose_and_momentum','physical_pickup_body','owner_disarmed','ammunition_transfer','player_collect','player_equip']+
        ([] if args.scenario=='death' else ['surviving_disarm','armour_and_light_hit_preserve_grip','no_phantom_shots','death_no_duplicate','restart_restores_grip'])}
    marker='COMBAT_LOOT_VERIFY_EVIDENCE '
    for line in log.splitlines():
        if marker in line:
            report['evidence']=json.JSONDecoder().raw_decode(line.split(marker,1)[1])[0]
    if args.scenario=='adaptive':
        report['checks']=[c for c in report['checks'] if c!='armour_and_light_hit_preserve_grip']
        report['checks']+=['equal_damage_different_retention','real_weapon_mass','collected_ammo_player_fire']
    if args.scenario=='rearm':
        report['checks']+=['reacquire_new_generation','stale_action_rejected','second_generation_release']
    if args.scenario=='action':
        report['checks']+=['native_parent_velocity_inheritance']
    if args.scenario=='data_override':
        report['checks']=['project_vfs_model_override','same_injury_retained_with_more_support','no_fixed_hp_release_rule']
    if args.scenario in ['adaptive','data_override']:
        report['checks']+=['supported_hit_preserves_capsule_ownership','actual_hit_reactions_observed']
    report['executable_sha256']=hashlib.sha256((ROOT/'target'/args.profile/'newviso.exe').read_bytes()).hexdigest()
    report['physics_provider_sha256']=hashlib.sha256((WORKSPACE/'pluginsRuntime/bullet-physics-0.3.0-release.dll').read_bytes()).hexdigest()
    save(cache/'acceptance.json',report)
    if not passed:raise RuntimeError(log[-10000:])
    print(json.dumps(report))
if __name__=='__main__':main()

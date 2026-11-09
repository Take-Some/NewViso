"""Native integration proof for opt-in combat; all test content is disposable."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time
ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent

def save(p,value):
    p.parent.mkdir(parents=True,exist_ok=True)
    p.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--profile',default='release',choices=['debug','release']);args=parser.parse_args()
    cache=ROOT/'.newviso/cache/combat-proof';fixture=cache/'fixture'
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
    manifest['project']['id']='combat-verification';manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
    save(fixture/'project.json',manifest)
    runtime=json.loads((fixture/manifest['files']['runtime']).read_text(encoding='utf-8'))
    authored=json.loads((WORKSPACE/'Projects/WeaponLab/config/runtime.json').read_text(encoding='utf-8'))['variables']['engine_combat']
    enemy=authored['enemies'][0].copy();enemy.update(id='combat.test.enemy',position=[0,.05,-5],patrol=[])
    enemy['weapon']='weapon.pistol.military'
    enemy['profile']={**enemy['profile'],'melee':None,'armed':True,'combat_cooldown_seconds':.4,'combat_damage':10,
        'combat':{**enemy['profile']['combat'],'melee_only':False,'magazine_capacity':2,'reserve':4,'reaction_seconds':.1,'reload_seconds':.7,'spread':0}}
    runtime['variables']['engine_combat']={**authored,'enemies':[enemy]}
    runtime['variables']['engine_player']['spawn_position']=[0,.05,6]
    runtime['variables']['engine_player']['required_streamed_mesh_colliders']=0
    runtime['window']={**runtime.get('window',{}),'title':'NorthStar combat verification','width':960,'height':600}
    save(fixture/manifest['files']['runtime'],runtime)
    overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True)
    shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
    shutil.copyfile(ROOT/'scripts/fixtures/combat_verify.ysc',overlay/'default_game.ysc')
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
    passed=code==0 and 'COMBAT_VERIFY_PASS' in log and not any(t in log for t in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
    report={'passed':passed,'exit_code':code,'seconds':round(time.time()-start,2),'male_models':len(imports['characters']),
        'checks':['native_detection','ranged_attack','player_damage','armour','reload_reserve','occlusion','projectile_cover_impact','search','reacquire','hit_reaction','ragdoll','death_weapon_drop','loot_physics_body','death_input_gate','encounter_restart','enemy_weapon_restore']}
    save(cache/'acceptance.json',report)
    if not passed:raise RuntimeError(log[-10000:])
    print(json.dumps(report))
if __name__=='__main__':main()

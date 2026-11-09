"""Native Endorphin acceptance with disposable project/VFS overlays."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time, re, sys

from import_endorphin_recovery import bind_joint_weights, read_pose_database

ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent
def save(path,value):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--profile',default='release',choices=['debug','release'])
    parser.add_argument('--scenario',default='default',choices=['default','data_override','active_control','functional_control','transfer_rest','active_rest','authority','weapon_aim','support_motion','protection','door','recovery_support_loss','recovery_obstructed','recovery_moving_support','recovery_moving_support_loss','recovery_moving_support_blocked'])
    parser.add_argument('--decoy',action='store_true')
    parser.add_argument('--rear-speed',type=float,default=1.2)
    parser.add_argument('--weapon',default='weapon.pistol.military')
    parser.add_argument('--ambient',action='store_true')
    parser.add_argument('--semantic-map',action='store_true')
    parser.add_argument('--authored-recovery',action='store_true')
    parser.add_argument('--pose-database',type=Path)
    parser.add_argument('--capture',action='store_true')
    parser.add_argument('--capture-target',choices=['npc','player'],default='npc')
    parser.add_argument('--trace',action='store_true')
    parser.add_argument('--model',type=Path)
    parser.add_argument('--run-tag',default='')
    parser.add_argument('--actor-index',type=int,default=0)
    parser.add_argument('--skinning',default='gpu',choices=['gpu','cpu'])
    args=parser.parse_args()
    if args.pose_database and not args.authored_recovery:
        parser.error("--pose-database requires --authored-recovery")
    if args.run_tag and not re.fullmatch(r'[a-z0-9_-]{1,32}',args.run_tag):raise ValueError('invalid proof run tag')
    if not 0<args.rear_speed<=20:raise ValueError('rear-speed must be finite and positive')
    variant=('-mapped' if args.semantic_map else '')+('-ambient' if args.ambient else '')
    if args.weapon!='weapon.pistol.military':variant+='-'+re.sub('[^a-z0-9_-]','-',args.weapon.lower())
    if args.rear_speed!=1.2:variant+='-rear-'+str(args.rear_speed).replace('.','-')
    if args.decoy:variant+='-decoy'
    if args.authored_recovery:variant+='-authored'
    if args.pose_database:variant+='-source-weights'
    cache=ROOT/'.newviso/cache'/('endorphin-'+args.scenario+('' if args.actor_index==0 else '-actor'+str(args.actor_index))+('' if args.skinning=='gpu' else '-cpu')+('-'+args.run_tag if args.run_tag else '')+variant+'-proof')
    fixture=cache/'fixture'
    if fixture.exists():shutil.rmtree(fixture)
    fixture.mkdir(parents=True)
    imports=json.loads((WORKSPACE/'Shared/Source/models/characters/gta/combat-peds.import.json').read_text(encoding='utf-8'))
    for character in imports['characters']:
        for resource in character['runtime_files'].values():
            assert hashlib.sha256((WORKSPACE/'Shared/Content'/resource['path']).read_bytes()).hexdigest()==resource['sha256'],resource['path']
    source=WORKSPACE/'Projects/WeaponLab'
    manifest=json.loads((source/'project.json').read_text(encoding='utf-8'))
    for key in ['runtime','environment','scene']:
        rel=manifest['files'][key];dest=fixture/rel
        dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/rel,dest)
    manifest['project']['id']='endorphin-verification'
    manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
    save(fixture/'project.json',manifest)
    runtime=json.loads((fixture/manifest['files']['runtime']).read_text(encoding='utf-8'))
    authored=runtime['variables']['engine_combat']
    if not 0<=args.actor_index<len(authored['enemies']):raise ValueError('actor index is outside authored enemies')
    enemy=authored['enemies'][args.actor_index].copy()
    enemy.update(id='endorphin.test.enemy',position=[0,.05,-5],patrol=[],weapon=args.weapon)
    enemy['profile']={**enemy['profile'],'max_health':100,'armour':0,'combat':None,'melee':None,
        'armed':False,'block_events':True,'combat_damage':0}
    runtime['variables']['engine_combat']={**authored,'enemies':[enemy]}
    if args.ambient:
        runtime['variables']['engine_combat']['enemies']=[]
        runtime['variables']['engine_peds']={'actors':[enemy]}
        runtime['variables']['endorphin_ambient_probe']=True
    runtime['variables']['engine_player']['spawn_position']=[0,.05,6]
    runtime['variables']['engine_player']['required_streamed_mesh_colliders']=0
    if args.scenario in ['protection','door']:
        player=runtime['variables']['engine_player'];player['controller']='shared';player['enabled']=True;player['camera_mode']='third_person'
        runtime['variables'].setdefault('engine_peds',{})['player_proxy']={'actor_id':player.get('scene_id','player.character'),'physics_entity':player.get('physics_entity',900000000),'center_offset':.9}
    if args.scenario=='door':
        runtime['variables']['engine_combat']['enemies'][0]['position']=[7,.05,-7]
        runtime['variables']['engine_doors']={'doors':[{'id':'verify.push_door','profile':'swing','hinge_position':[0,1.06,-5],'size':[1,2.1,.06],'rotation_degrees':[0,0,0],'hinge_side':-1,'joint_ids':[900082001,900082002],'frame':True},{'id':'verify.reverse_door','profile':'swing','hinge_position':[4,1.06,-5],'size':[1,2.1,.06],'rotation_degrees':[0,0,0],'hinge_side':-1,'joint_ids':[900082003,900082004],'frame':True}]}

    model=json.loads((WORKSPACE/'Shared/Content/physics/endorphin/humanoid.json').read_text(encoding='utf-8'))
    if args.model:
        model=json.loads(args.model.read_text(encoding='utf-8'))
        save(fixture/'assets/physics/endorphin/humanoid.json',model)
    if args.scenario=='data_override':
        model['recovery']['response_time_scale']*=2
        save(fixture/'assets/physics/endorphin/humanoid.json',model)
    if args.authored_recovery:
        clip_ref = f"animations/characters/gta/reactions/{Path(enemy['model']).stem}.getup.ycd@back"
        sample_times = [0.1, 0.2, 0.35]
        blend_seconds = 0.12
        weights = {'SKEL_Pelvis': 5, 'SKEL_Head': 3, 'SKEL_L_Hand': 1,
                   'SKEL_R_Hand': 1, 'SKEL_L_Foot': 2, 'SKEL_R_Foot': 2,
                   'SKEL_L_Clavicle': 2, 'fixture.absent.zero_weight': 0}
        joints = [{'joint': joint, 'weight': weight} for joint, weight in weights.items()]
        if args.pose_database:
            model_name = Path(enemy['model']).stem
            skeleton = WORKSPACE/'Shared/Content'/Path(enemy['model']).with_name(f'{model_name}.skeleton.ymt')
            joints = bind_joint_weights(read_pose_database(args.pose_database), skeleton)
        model['recovery_match'] = {
            'joints': joints,
            'clips': {clip_ref: {'sample_times_seconds': sample_times, 'score_multiplier': 0.5,
                                 'blend_seconds': blend_seconds}},
        }
        runtime['variables']['endorphin_expected_match'] = {
            'clip_ref': clip_ref, 'times': sample_times, 'blend_seconds': blend_seconds,
            'marker_count': sum(marker['weight'] > 0 for marker in joints),
        }
        save(fixture/'assets/physics/endorphin/humanoid.json',model)
    if args.semantic_map:
        # The runtime controller receives opaque author names, mapped by data to this installed rig.
        names=set()
        for part in model['rig']['parts']:
            for key in ['joint','end_joint','anchor_joint']:
                if part.get(key):names.add(part[key])
        names.update(m['joint'] for m in model.get('recovery_match', {}).get('joints', []))
        weapon=model.get('weapon_rig')
        if weapon:
            names.add(weapon['eye_joint'])
            for side in ['right','left']:
                a=weapon[side]
                names.update(a[k] for k in ['upper_joint','root_joint','tip_joint','weapon_joint'])
                for g in a['grips'].values():names.update(j['joint'] for j in g['joint_rotations'])
        mapping={name:'opaque.binding.'+str(i) for i,name in enumerate(sorted(names))}
        def rewrite(v):
            if isinstance(v,str):return mapping.get(v,v)
            if isinstance(v,list):return [rewrite(x) for x in v]
            if isinstance(v,dict):return {k:rewrite(x) for k,x in v.items()}
            return v
        grip=rewrite(json.loads((WORKSPACE/'Shared/Content/physics/peds/humanoid_grip.json').read_bytes()))
        save(fixture/'assets/physics/peds/humanoid_grip.json',grip)
        model=rewrite(model);model['joint_map']={alias:actual for actual,alias in mapping.items()}
        save(fixture/'assets/physics/endorphin/humanoid.json',model)
    runtime['variables']['endorphin_recovery_context']={'recovery_support_loss':'support_loss','recovery_obstructed':'obstructed','recovery_moving_support':'moving','recovery_moving_support_loss':'moving_loss','recovery_moving_support_blocked':'moving_blocked'}.get(args.scenario,'')
    runtime['variables']['endorphin_capture']=args.capture
    runtime['variables']['endorphin_capture_target']=args.capture_target
    runtime['variables']['endorphin_decoy_probe']=args.decoy
    runtime['variables']['endorphin_expected_weapon']=args.weapon
    runtime['variables']['endorphin_rear_speed']=args.rear_speed
    runtime['variables']['endorphin_expected_time_scale']=model['recovery']['response_time_scale']
    runtime['variables']['endorphin_require_standing']=args.scenario!='functional_control'
    runtime['variables']['endorphin_rest_gravity']=0.000001 if args.scenario=='active_rest' else 0
    runtime['variables']['endorphin_trace_controls']=args.trace
    runtime['window']={**runtime.get('window',{}),'title':'NorthStar Endorphin verification','width':960,'height':600}
    save(fixture/manifest['files']['runtime'],runtime)
    overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True)
    shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
    fixtures={'weapon_aim':'weapon_aim_verify.ysc','authority':'endorphin_authority_verify.ysc',
        'support_motion':'endorphin_support_verify.ysc','protection':'endorphin_protection_verify.ysc','door':'door_verify.ysc',
        'active_control':'endorphin_control_verify.ysc','functional_control':'endorphin_control_verify.ysc',
        'transfer_rest':'endorphin_rest_verify.ysc','active_rest':'endorphin_rest_verify.ysc'}
    shutil.copyfile(ROOT/'scripts/fixtures'/fixtures.get(args.scenario,'endorphin_verify.ysc'),overlay/'default_game.ysc')
    env=os.environ.copy()
    env['NEWVISO_CPU_SKINNING']='1' if args.skinning=='cpu' else '0'
    env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
        NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),
        VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),
        NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
    executable=ROOT/'target'/args.profile/'newviso.exe'
    start=time.time()
    with (cache/'native.log').open('wb') as log:
        proc=subprocess.Popen([str(executable),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','9000'],
            cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
        (cache/'native.pid').write_text(str(proc.pid))
        captures=set()
        while proc.poll() is None:
            if time.time()-start>180:
                proc.terminate();proc.wait(timeout=10);raise RuntimeError('Endorphin native proof timed out')
            if args.capture:
                text=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
                for name in ['npc','player','door']:
                    if name not in captures and 'INTERACTION_CAPTURE '+name in text:
                        captures.add(name)
                        capture=subprocess.run([sys.executable,str(ROOT/'scripts/capture_game_window.py'),str(cache),name+'.png'],capture_output=True,text=True)
                        (cache/(name+'-capture.json')).write_bytes(json.dumps({'exit_code':capture.returncode,'stdout':capture.stdout,'stderr':capture.stderr}).encode())
            time.sleep(.03)
        code=proc.returncode
    log=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
    passed=code==0 and 'ENDORPHIN_VERIFY_PASS' in log and not any(t in log for t in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
    if args.capture:passed=passed and (cache/((args.capture_target if args.scenario=='protection' else 'door')+'.png')).exists()
    report={'passed':passed,'exit_code':code,'seconds':round(time.time()-start,2),'scenario':args.scenario,'actor_index':args.actor_index,'skinning':args.skinning,'build_profile':args.profile,
        'executable_sha256':hashlib.sha256(executable.read_bytes()).hexdigest(),
        'checks':['living_articulation','airborne_articulation','energy_landing','pose_matched_getup','safe_root_placement','physical_pose_blend','world_clearance','physical_settling','getup_handoff','death','reset_cleanup','single_weapon_transfer','vfs_model','bone_lengths','bone_scale','rigid_bases','native_anchor_integrity','no_matrix_interpolation','no_vertex_interpolation']}
    if args.scenario in ['active_control','functional_control']:report['checks']=['physical_upright_control','anatomical_limits','local_motor_capacity','damage_persistence','armour_absorption','death_disables_motors','reset_cleanup','bone_integrity']
    report['semantic_map']=args.semantic_map
    report['ambient']=args.ambient
    report['weapon']=args.weapon
    report['rear_speed']=args.rear_speed
    report['decoy']=args.decoy
    if args.scenario=='weapon_aim':report['checks']=['rear_shot_rejected','no_ammo_spent','body_turn','real_muzzle','geometry_bound_arms']
    if args.scenario=='protection':report['checks']=['npc_and_player_reach','incoming_surface','release_handoff','native_muscles','death_passive','bone_lengths']
    if args.scenario=='door':report['checks']=['shared_door','player_and_npc_contact_opens','hinge_integrity','angular_stops','unforced_rest']
    if args.scenario=='support_motion':report['checks']=['surface_local_foot_targets','translation_rotation','live_point_velocity','stopping','support_retirement','rigid_skeleton','native_motors']
    if args.scenario=='authority':report['checks']=['upright_guard','physical_ownership_blocks_voluntary_combat','transfer_cancels_inflight_strike','passive_death','reset_handoff','supported_flinch_locomotion','recovered_combat_authority']
    if args.scenario=='functional_control':report['checks'][0]='physical_muscle_drives'
    if args.scenario in ['transfer_rest','active_rest']:report['checks']=['stationary_articulation','no_artificial_energy','owner_obstruction_filter','owner_collision_separation','real_release_momentum']
    if args.scenario=='active_rest':report['checks'].append('active_pose_drives')
    marker='ENDORPHIN_VERIFY_EVIDENCE '
    for line in log.splitlines():
        if marker in line:report['evidence']=json.JSONDecoder().raw_decode(line.split(marker,1)[1])[0]
    provider=WORKSPACE/'pluginsRuntime/bullet-physics-0.3.0-release.dll'
    if provider.exists():report['physics_provider_sha256']=hashlib.sha256(provider.read_bytes()).hexdigest()
    errors=[line for line in log.splitlines() if any(t in line for t in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])]
    if errors:report['failure']=errors[0][:2500]
    save(cache/'acceptance.json',report)
    if not passed:raise RuntimeError(report.get('failure','native proof did not emit PASS')+'\nNative log: '+str(cache/'native.log'))
    print(json.dumps(report))
if __name__=='__main__':main()

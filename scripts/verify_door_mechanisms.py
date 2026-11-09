"""Native locks, latches, finite effort, NPC passage and restart verification."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,time,argparse
ROOT=Path(__file__).resolve().parents[1];WORKSPACE=ROOT.parent
def save(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--run-tag',default='current');parser.add_argument('--only',choices=['exercise','restart','obstruction','impact','broken_restart','egress']);args=parser.parse_args()
 cache=ROOT/'.newviso/cache/door-mechanisms-work/20261027'/('native-'+args.run_tag);fixture=cache/'fixture'
 if fixture.exists():shutil.rmtree(fixture)
 fixture.mkdir(parents=True)
 source=WORKSPACE/'Projects/WeaponLab';manifest=json.loads((source/'project.json').read_bytes())
 for k in ['runtime','environment','scene']:
  dst=fixture/manifest['files'][k];dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/manifest['files'][k],dst)
 manifest['project']['id']='door-mechanisms-verification';manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
 save(fixture/'project.json',manifest)
 runtime=json.loads((fixture/manifest['files']['runtime']).read_bytes());v=runtime['variables']
 actors=[]
 for i,e in enumerate(v['engine_combat']['enemies'][:2]):
  a={k:e[k] for k in ['model','locomotion','rootLift']}
  a.update(id='door.npc.'+str(i),position=[-3-i*2,.05,-2],group='civilian',profile={'max_health':100,'armour':0,'combat':None,'melee':None,'armed':False,'block_events':True})
  actors.append(a)
 assert len(actors)==2
 v['engine_combat']={'enabled':False};v['engine_peds']={'actors':actors,'player_proxy':{'actor_id':'player.character','physics_entity':900000000,'center_offset':.9}}
 v['engine_player'].update(controller='shared',enabled=True,camera_mode='third_person',spawn_position=[.9,.05,-4.1],required_streamed_mesh_colliders=0)
 v['engine_items']['definitions'] += [{'id':'key.office','display_name':'Office key','category':'key','max_stack':1},{'id':'key.other','display_name':'Other key','category':'key','max_stack':1}]
 v['engine_items']['pickups']=[]
 v['engine_doors']={'doors':[
 {'id':'verify.locked','profile':'swing','hinge_position':[0,1.06,-5],'size':[1.2,2.1,.06],'rotation_degrees':[0,0,0],'hinge_side':-1,'joint_ids':[900088001,900088002],'frame':True,'mechanism':{'latch':'handle','initially_locked':True,'key_item':'key.office','free_egress_side':-1}},
 {'id':'verify.pull','profile':'pull','hinge_position':[4,1.06,-5],'size':[1.2,2.1,.06],'rotation_degrees':[0,0,0],'hinge_side':-1,'joint_ids':[900088003,900088004],'frame':True,'mechanism':{'latch':'handle'} }
 ],'profiles':{'pull':{'angle_limits_degrees':[0,100]}}}
 # Profiles merge by key, so copy the actual shared swing profile before overriding the hinge envelope.
 import xml.etree.ElementTree as ET
 def decode(n):
  t=n.get('type')
  if len(n) and t!='array':return {c.tag:decode(c) for c in n}
  if t=='array':return [decode(c) for c in n]
  x=n.get('value')
  if x in ['true','false']:return x=='true'
  try:return float(x)
  except (TypeError,ValueError):return x
 defaults=ET.fromstring((WORKSPACE/'Shared/Content/config/engine/runtime.defaults.xml').read_bytes())
 swing=decode(defaults.find('.//engine_doors/profiles/swing'))
 assert swing and 'mass_kg' in swing
 v['engine_doors']['profiles']['pull']={**swing,'angle_limits_degrees':[0,100]}
 runtime['window'].update(title='NorthStar door mechanisms verification',width=960,height=600)
 runtime['world_persistence']={'save_path':'saves/doors.json','load_on_start':True,'save_on_shutdown':True,'autosave_interval_seconds':0}
 overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True)
 shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
 shutil.copyfile(ROOT/'scripts/fixtures/door_mechanisms_verify.ysc',overlay/'default_game.ysc')
 env=os.environ.copy();env.update(NEWVISO_SCENE_MATERIAL_DIAG='1',NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
 exe=ROOT/'target/release/newviso.exe';digest=hashlib.sha256(exe.read_bytes()).hexdigest();runs=[]
 modes=[args.only] if args.only else ['exercise','restart','obstruction','impact','broken_restart','egress']
 for mode in modes:
  if mode in ['obstruction','impact','egress']:
   shutil.rmtree(fixture/'saves',ignore_errors=True)
  v['engine_doors']['doors'][1]['hinge_side']=1 if mode=='egress' else -1
  v['engine_doors']['doors'][1]['hinge_position']=[5.2,1.06,-5] if mode=='egress' else [4,1.06,-5]
  v['door_verification_mode']=mode;save(fixture/manifest['files']['runtime'],runtime)
  start=time.time();log=cache/(mode+'.log')
  with log.open('wb') as out:
   p=subprocess.Popen([str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','15000'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
   (cache/'native.pid').write_text(str(p.pid))
   captured=False
   while p.poll() is None:
    if time.time()-start>180:p.terminate();p.wait(timeout=10);raise RuntimeError('native door timeout')
    if not captured and mode in ['exercise','egress'] and ('DOOR_CAPTURE '+('right' if mode=='egress' else 'textured')) in log.read_text(encoding='utf-8',errors='replace'):
     captured=True
     cap=subprocess.run([os.sys.executable,str(ROOT/'scripts/capture_game_window.py'),str(cache),'textured-right-door.png' if mode=='egress' else 'textured-door.png','NorthStar door mechanisms verification'],capture_output=True,text=True)
     save(cache/'capture.json',{'exit_code':cap.returncode,'stdout':cap.stdout,'stderr':cap.stderr})
    time.sleep(.05)
   code=p.returncode
  text=log.read_text(encoding='utf-8',errors='replace')
  passed=code==0 and ('DOOR_MECHANISMS_PASS '+mode) in text and not any(x in text for x in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
  material_bindings=None
  if mode in ['exercise','egress']:
   snapshots=[json.loads(line.split('pre-shutdown runtime state: ',1)[1])for line in text.splitlines()if 'pre-shutdown runtime state: 'in line]
   if snapshots:
    instances=snapshots[-1]['scene']['main_view_geometry']['instances']
    door=next((i for i in instances if i.get('id')==('verify.pull' if mode=='egress' else 'verify.locked')),{})
    material_bindings=door.get('material_bindings')
   passed=passed and bool(material_bindings) and all(m.get('base_color_texture') and m.get('gpu_ready')for m in material_bindings)
   passed=passed and (cache/('textured-right-door.png' if mode=='egress' else 'textured-door.png')).exists()
  evidence=[line.split('DOOR_EVIDENCE ',1)[1] for line in text.splitlines() if 'DOOR_EVIDENCE ' in line]
  runs.append({'mode':mode,'passed':passed,'exit_code':code,'seconds':round(time.time()-start,2),'evidence':evidence,'materials':material_bindings})
  save(cache/'acceptance.json',{'passed':all(r['passed'] for r in runs) and len(runs)==len(modes),'executable_sha256':digest,'runs':runs})
  if not passed:raise RuntimeError(text[-10000:])
 print(json.dumps({'passed':True,'executable_sha256':digest,'runs':runs}))
if __name__=='__main__':main()

"""Native engine-owned locomotion/audio proof, without project audio installation."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[1];WORKSPACE=ROOT.parent

def save(path,value):
 path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--controller',choices=['shared','project'],default='shared');parser.add_argument('--view',choices=['first_person','third_person'],default='first_person');parser.add_argument('--npcs',action='store_true');parser.add_argument('--firstfps',action='store_true');args=parser.parse_args()
 cache=ROOT/'.newviso/cache'/('character-motion-'+args.controller+('' if args.view=='first_person' else '-'+args.view)+('-firstfps' if args.firstfps else '')+('-npcs' if args.npcs else '')+'-proof');fixture=cache/'fixture'
 fixture.mkdir(parents=True,exist_ok=True)
 for name in ['acceptance.json','first-person.png']:(cache/name).unlink(missing_ok=True)
 source=WORKSPACE/'Projects/WeaponLab';manifest=json.loads((source/'project.json').read_bytes())
 for key in ['runtime','environment','scene']:
  rel=manifest['files'][key];dest=fixture/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/rel,dest)
 manifest['project']['id']='character-motion-verification';manifest['scripts']['entrypoint']='scripts/newviso/default_game.ysc'
 save(fixture/'project.json',manifest)
 runtime=json.loads((fixture/manifest['files']['runtime']).read_bytes())
 v=runtime['variables']
 v['engine_player'].update(controller=args.controller,camera_mode=args.view,walk_speed=1.5,sprint_speed=4.6,spawn_position=[0,.05,6])
 enemies=v.get('engine_combat',{}).get('enemies',[])
 v['motion_verification_npcs']=[]
 if args.npcs:
  actors=[]
  for i,enemy in enumerate(enemies[:2]):
   actors.append({k:enemy[k]for k in ['model','locomotion','rootLift']})
   actors[-1].update(id='motion.npc.'+str(i),position=[-3+i*6,.05,2],group='civilian',profile={'max_health':100,'combat':None,'armed':False})
  assert len(actors)==2
  v['engine_peds']={'actors':actors};v['motion_verification_npcs']=[p['id']for p in actors]
 else:v['engine_peds']={'actors':[]}
 v['engine_combat']={'enabled':False}
 runtime['window'].update(title='NorthStar character motion verification',width=960,height=600)
 save(fixture/manifest['files']['runtime'],runtime)
 overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True,exist_ok=True)
 shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
 shutil.copyfile(ROOT/'scripts/fixtures'/('character_motion_fps_verify.ysc' if args.firstfps else 'character_motion_verify.ysc'),overlay/'default_game.ysc')
 if args.firstfps:
  assert args.controller=='project'
  # Finite air control retains sprint momentum. Use the clear side lane,
  # keeping the player clear of the yard's centreline range fixtures on runout.
  scene_path=fixture/manifest['files']['scene'];scene=json.loads(scene_path.read_bytes())
  camera=next(e for e in scene['entities']if e.get('kind')=='camera')
  camera['transform']['position'][0]=10;camera['transform']['target'][0]=10
  save(scene_path,scene)
  runtime['variables']['engine_player']['spawn_position'][0]=10
  save(fixture/manifest['files']['runtime'],runtime)
  renderer_guard=WORKSPACE/'PluginsSrc/VulkanRenderer/newengine-modules-render-vulkan-ash/tests/verify_compiled_coverage.py'
  shader_check=subprocess.run([sys.executable,str(renderer_guard),'--provider',str(WORKSPACE/'pluginsRuntime/vulkan-renderer-0.37.7-release.dll')],capture_output=True,text=True)
  if shader_check.returncode:raise RuntimeError('compiled first-person coverage contract: '+shader_check.stdout+shader_check.stderr)
  shutil.copytree(WORKSPACE/'Projects/FirstFPS/content/particles',fixture/'content/particles',dirs_exist_ok=True)
  for source_script in (WORKSPACE/'Projects/FirstFPS/scripts/fps').rglob('*.ysc'):
   dest=fixture/'assets/scripts/fps'/source_script.relative_to(WORKSPACE/'Projects/FirstFPS/scripts/fps')
   dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source_script,dest)
 env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
 NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),
 VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),
 NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'),NEWVISO_AUDIO_CONTACT_DIAG='1',NEWVISO_SCENE_MATERIAL_DIAG='1')
 exe=ROOT/'target/release/newviso.exe';digest=hashlib.sha256(exe.read_bytes()).hexdigest()
 start=time.time();command=[str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','5000']
 with (cache/'native.log').open('wb') as log:
  proc=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT);(cache/'native.pid').write_text(str(proc.pid))
  capture_done=False
  while proc.poll() is None:
   if time.time()-start>150:proc.terminate();proc.wait(timeout=10);raise RuntimeError('native timeout')
   text=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
   if not capture_done and 'MOTION_CAPTURE first_person' in text:
    capture_done=True
    capture=subprocess.run([sys.executable,str(ROOT/'scripts/capture_game_window.py'),str(cache),'first-person.png','NorthStar character motion verification'],capture_output=True,text=True)
    save(cache/'capture.json',{'exit_code':capture.returncode,'stdout':capture.stdout,'stderr':capture.stderr})
   time.sleep(.05)
 code=proc.returncode
 text=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
 # The per-frame script payload intentionally omits heavy render diagnostics.
 # Material verification reads the complete native snapshot, not the slim script frame.
 material_bindings=None
 if args.firstfps:
  snapshots=[json.loads(line.split('pre-shutdown runtime state: ',1)[1])for line in text.splitlines()if 'pre-shutdown runtime state: 'in line]
  if snapshots:
   instances=snapshots[-1]['scene']['main_view_geometry']['instances']
   player=next((i for i in instances if i['id']=='player.character'),{})
   material_bindings=player.get('material_bindings')
 contacts={kind:text.count('event='+kind+' ')for kind in ['Footstep','Lift','Landing']}
 passed=code==0 and 'CHARACTER_MOTION_VERIFY_PASS' in text and all(contacts[k]>0 for k in contacts)
 passed=passed and not any(x in text for x in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED'])
 if args.firstfps:passed=passed and bool(material_bindings) and all(m.get('base_color_texture') and m.get('gpu_ready') for m in material_bindings)
 report={'renderer_sha256':hashlib.sha256((WORKSPACE/'pluginsRuntime/vulkan-renderer-0.37.7-release.dll').read_bytes()).hexdigest(),'passed':passed,'material_bindings':material_bindings,'exit_code':code,'controller':args.controller,'view':args.view,'npcs':args.npcs,'firstfps':args.firstfps,'seconds':round(time.time()-start,2),'contacts':contacts,'executable_sha256':digest,'capture':(cache/'first-person.png').exists()}
 save(cache/'acceptance.json',report);print(json.dumps(report))
 if not passed:raise RuntimeError(text[-6500:])
if __name__=='__main__':main()

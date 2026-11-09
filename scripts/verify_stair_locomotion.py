"""Exercise real Shared/FirstFPS input and two native skeletons on individual stair treads."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,time,ast,re,math
ROOT=Path(__file__).resolve().parents[1];WORKSPACE=ROOT.parent
def save(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--controller',choices=['shared','project'],default='shared');parser.add_argument('--tag',default='current');parser.add_argument('--trace',action='store_true');parser.add_argument('--profile',choices=['debug','release'],default='release');parser.add_argument('--adapter',choices=['FirstFPS','Island','ProceduralMap'],default='FirstFPS');parser.add_argument('--case',choices=['stairs','wall','ceiling','original'],default='stairs');args=parser.parse_args()
 cache=ROOT/'.newviso/cache/stair-locomotion-work'/args.tag;fixture=cache/'fixture';fixture.mkdir(parents=True,exist_ok=True)
 for stale in ['acceptance.json','stairs-upper.png','stairs-middle.png']:
  p=cache/stale
  if p.exists():p.unlink()
 source=WORKSPACE/'Projects/WeaponLab';manifest=json.loads((source/'project.json').read_bytes())
 for k in ['runtime','environment','scene']:
  p=fixture/manifest['files'][k];p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/manifest['files'][k],p)
 manifest['project']['id']='stair-locomotion-verification';save(fixture/'project.json',manifest)
 runtime=json.loads((fixture/manifest['files']['runtime']).read_bytes());v=runtime['variables']
 adapter_root=WORKSPACE/'Projects'/args.adapter
 adapter_manifest=json.loads((adapter_root/'project.json').read_bytes())
 adapter_runtime=json.loads((adapter_root/adapter_manifest['files']['runtime']).read_bytes())
 for authored in adapter_runtime.get('startup_commands',[]):
  if authored.get('op')!='scene.render.configure':continue
  for command in runtime.get('startup_commands',[]):
   if command.get('op')!='scene.render.configure':continue
   settings=command.setdefault('settings',{})
   for key,value in authored.get('settings',{}).items():
    if key.endswith('_capacity'):settings[key]=max(settings.get(key,0),value)
 actors=[]
 for i,e in enumerate(v['engine_combat']['enemies'][:2]):
  a={k:e[k]for k in ['model','locomotion','rootLift']};a.update(id='stair.npc.'+str(i),position=[3+i*3,.05,2],group='civilian',profile={'max_health':100,'combat':None,'armed':False,'block_events':True});actors.append(a)
 v['stair_verification_case']=args.case;v['stair_verification_trace']=args.trace
 v['engine_peds']={'actors':actors};v['stair_verification_npcs']=[a['id']for a in actors];v['engine_combat']={'enabled':False};v['engine_doors']={'doors':[]}
 v['engine_player'].update(controller=args.controller,camera_mode='third_person',spawn_position=[0,.05,2],required_streamed_mesh_colliders=0,walk_speed=1.5)
 runtime['window'].update(title='NorthStar stair locomotion verification',width=960,height=600);save(fixture/manifest['files']['runtime'],runtime)
 scene={'schema':'newviso.scene.v1','version':1,'title':'Individual stair treads','entities':[
 {'kind':'camera','name':'PlayerCamera','transform':{'position':[0,1.7,2],'target':[0,1.7,-3],'up':[0,1,0]},'camera':{'projection':'perspective','fov_y_degrees':72,'near':.04,'far':250}}]}
 def box(name,min,max,color):
  scene['entities'].append({'kind':'mesh','name':name,'transform':{'position':[(a+b)/2 for a,b in zip(min,max)],'rotation_degrees':[0,0,0],'scale':[b-a for a,b in zip(min,max)]},'mesh':{'primitive':'cube'},'material':{'base_color':color},'collider':{'solid':True}})
 box('ground',[-1.5,-.5,0],[7.5,0,8],[.25,.28,.3,1])
 for i in range(8):box('tread.'+str(i),[-1.5,0,-(i+1)*.32],[7.5,(i+1)*.18,-i*.32],[.55+i*.02,.4,.25,1])
 box('landing',[-1.5,0,-6],[7.5,1.44,-2.56],[.5,.5,.5,1])
 if args.case=='wall':box('high-wall',[-1.5,0,-.3],[7.5,2.5,0],[.6,.1,.1,1])
 if args.case=='ceiling':box('low-ceiling',[-1.5,1.87,-.7],[7.5,2.2,.7],[.6,.1,.1,1])
 if args.case=='original':
  assert args.controller=='project'
  authored=json.loads((WORKSPACE/'Projects/FirstFPS/scenes/lifeinvader_rsc7.scene.json').read_bytes())
  stair=next(e for e in authored['entities']if e.get('name','').endswith('.v_9_steps'))
  collider=next(e for e in authored['entities']if e.get('name')=='rsc7.collision.v_faceoffice')
  origin=stair['transform']['position'];angle=math.radians(stair['transform']['rotation_degrees'][1]);co,si=math.cos(angle),math.sin(angle)
  def point(x,y,z):return [origin[0]+co*x+si*z,y,origin[2]-si*x+co*z]
  axis=[co,0,-si];floor=.024505615234375;upper=origin[1]-3.27978515625
  start=point(-3.5,floor,1.7);goal=point(.8,upper,1.7)
  course={'start':start,'axis':axis,'length':4.3,'upper_y':upper,'goals':[],'returns':[]}
  for i,z in enumerate([.75,2.65]):
   pos=point(-3.5,floor+.05,z);actors[i]['position']=pos
   course['goals'].append(point(.8,upper,z));course['returns'].append(point(-3.5,floor,z))
  v['original_stair_course']=course;v['engine_player'].update(spawn_position=[start[0],floor+.05,start[2]],required_streamed_mesh_colliders=1)
  eye=[start[0],floor+.05+1.65,start[2]];camera=scene['entities'][0]
  camera['transform'].update(position=eye,target=[eye[i]+axis[i]*8 for i in range(3)])
  scene['entities']=[camera,stair,collider]
  for e in [stair,collider]:
   rel=e['mesh']['asset'];dest=fixture/'assets'/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(WORKSPACE/'Projects/FirstFPS/assets'/rel,dest)
  rel='textures/lifeinvader/v_9_v_faceoffice.ytd';dest=fixture/'assets'/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(WORKSPACE/'Projects/FirstFPS/assets'/rel,dest)
  save(fixture/manifest['files']['runtime'],runtime)
 # Preserve the selected adapter's authored initialization dependencies.
 if args.adapter=='Island':
  island=WORKSPACE/'Projects/Island'
  authored=json.loads((island/'scenes/island.scene.json').read_bytes())
  for entity in authored['entities']:
   if entity.get('name') not in ['BuoyancyRaft','FloatingCargo']:continue
   scene['entities'].append(entity)
   rel=entity['mesh']['asset'];dest=fixture/'assets'/rel;dest.parent.mkdir(parents=True,exist_ok=True)
   source_asset=island/'assets'/rel
   if not source_asset.exists():source_asset=WORKSPACE/'Shared/Content'/rel
   shutil.copyfile(source_asset,dest)
 save(fixture/manifest['files']['scene'],scene)
 overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True,exist_ok=True);shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc');shutil.copyfile(ROOT/'scripts/fixtures'/('original_stairs_verify.ysc' if args.case=='original' else 'stair_locomotion_verify.ysc'),overlay/'default_game.ysc')
 # Import resolution is shared for both branches, even when only Shared owns input.
 for s in (WORKSPACE/'Projects'/args.adapter/'scripts/fps').rglob('*.ysc'):
  p=fixture/'assets/scripts/fps'/s.relative_to(WORKSPACE/'Projects'/args.adapter/'scripts/fps');p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(s,p)
 shutil.copytree(WORKSPACE/'Projects/FirstFPS/content/particles',fixture/'content/particles',dirs_exist_ok=True)
 env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'),NEWVISO_AUDIO_CONTACT_DIAG='1')
 exe=ROOT/'target'/args.profile/'newviso.exe';digest=hashlib.sha256(exe.read_bytes()).hexdigest();start=time.time()
 log=cache/'native.log'
 with log.open('wb')as out:
  p=subprocess.Popen([str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','18000'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
  (cache/'native.pid').write_text(str(p.pid))
  captured=set()
  while p.poll() is None:
   if time.time()-start>180:p.terminate();p.wait(timeout=10);break
   text=log.read_text(encoding='utf-8',errors='replace')
   label=next((v for v in ['middle','upper'] if v not in captured and 'STAIR_CAPTURE '+v in text),None)
   if label is not None:
    captured.add(label)
    helper=(ROOT/'.newviso/cache/combat-detail-capture.py').read_text(encoding='utf-8');tree=ast.parse(helper)
    embedded=[n.args[0].value for n in ast.walk(tree)if isinstance(n,ast.Call)and isinstance(n.func,ast.Name)and n.func.id=='exec'][0]
    embedded="\n".join("c=pathlib.Path("+repr(cache.as_posix())+")" if line.startswith("c=pathlib.Path(") else line for line in embedded.splitlines()).replace('NorthStar fist verification','NorthStar stair locomotion verification').replace('fist-window.png','stairs-'+label+'.png')
    try:exec(embedded,{'__name__':'capture'})
    except (Exception,SystemExit) as e:(cache/'capture-error.txt').write_text(str(e),encoding='utf-8')
   time.sleep(.1)
  code=p.returncode
 text=log.read_text(encoding='utf-8',errors='replace')
 samples=[json.loads(l.split('STAIR_SAMPLE ',1)[1])for l in text.splitlines()if 'STAIR_SAMPLE 'in l]
 result=[json.loads(l.split('STAIR_VERIFY_PASS ',1)[1])for l in text.splitlines()if 'STAIR_VERIFY_PASS 'in l]
 report={'passed':code==0 and bool(result) and not any(x in text for x in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED']),'exit_code':code,'controller':args.controller,'adapter':args.adapter,'profile':args.profile,'case':args.case,'executable_sha256':digest,'seconds':time.time()-start,'sample_count':len(samples),'result':result}
 save(cache/'samples.json',samples);save(cache/'acceptance.json',report);print(json.dumps(report))
 if not report['passed']:raise RuntimeError(text[-7000:])
if __name__=='__main__':main()

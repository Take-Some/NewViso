"""Native proof: generic scene entities and rigid props share real stair collision geometry."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,time
ROOT=Path(__file__).resolve().parents[1];WORKSPACE=ROOT.parent
def save(p,v):
 p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def main():
 a=argparse.ArgumentParser();a.add_argument('--profile',choices=['debug','release'],default='release');a.add_argument('--tag',default='current');a.add_argument('--trace',action='store_true');args=a.parse_args()
 cache=ROOT/'.newviso/cache/entity-stairs-work'/args.tag;fixture=cache/'fixture';fixture.mkdir(parents=True,exist_ok=True)
 for name in ['acceptance.json','stairs-middle.png','stairs-upper.png']+['stairs-gait'+str(i)+'.png'for i in range(5)]:
  (cache/name).unlink(missing_ok=True)
 source=WORKSPACE/'Projects/WeaponLab';manifest=json.loads((source/'project.json').read_bytes())
 for k in ['runtime','environment','scene']:
  p=fixture/manifest['files'][k];p.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/manifest['files'][k],p)
 manifest['project']['id']='generic-entity-stairs-verification';save(fixture/'project.json',manifest)
 runtime=json.loads((fixture/manifest['files']['runtime']).read_bytes());old=runtime['variables']
 models=[{k:e[k]for k in ['model','locomotion']}for e in old['engine_combat']['enemies'][:2]];models.append(models[0])
 props=[]
 for i,shape in enumerate([{'kind':'box','half_extents':[.06,.08,.06]},{'kind':'sphere','radius':.06},{'kind':'cylinder','radius':.06,'half_height':.08}]):
  tread=1+i*2;height=(tread+1)*.18;bottom=.06 if i==1 else .08
  props.append({'entity':920000010+i,'shape':shape,'position':[9+i,height+.6,-(tread+.5)*.32],'expected_y':height+bottom})
 props.append({'entity':920000020,'shape':{'kind':'sphere','radius':.06},'position':[9,.063,1.2],'velocity':[0,0,-6],'riser':True})
 runtime['variables']={'engine_player':{'enabled':False},'engine_peds':{'actors':[]},'engine_combat':{'enabled':False},'engine_doors':{'doors':[]},
  'engine_items':{'owner_id':'disabled','pickups':[],'definitions':[]},'engine_world':old['engine_world'],
  'entity_stair_models':models,'entity_stair_props':props,'entity_stair_trace':args.trace}
 runtime['window'].update(title='NorthStar entity stairs verification',width=960,height=600);save(fixture/manifest['files']['runtime'],runtime)
 scene={'schema':'newviso.scene.v1','version':1,'title':'Generic entities on individual treads','entities':[
 {'kind':'camera','name':'PlayerCamera','transform':{'position':[-5,4.6,2],'target':[3.5,1,-1.5],'up':[0,1,0]},'camera':{'projection':'perspective','fov_y_degrees':65,'near':.04,'far':250}}]}
 def box(name,lo,hi,color):
  scene['entities'].append({'kind':'mesh','name':name,'transform':{'position':[(a+b)/2 for a,b in zip(lo,hi)],'rotation_degrees':[0,0,0],'scale':[b-a for a,b in zip(lo,hi)]},'mesh':{'primitive':'cube'},'material':{'base_color':color},'collider':{'solid':True}})
 box('ground',[-1.5,-.5,0],[12.5,0,8],[.25,.28,.3,1])
 for i in range(8):box('tread.'+str(i),[-1.5,0,-(i+1)*.32],[12.5,(i+1)*.18,-i*.32],[.55+i*.02,.4,.25,1])
 box('landing',[-1.5,0,-6],[12.5,1.44,-2.56],[.5,.5,.5,1])
 for i in range(2):
  component={'foot_placement_profile':'physics/endorphin/feet_humanoid.json'}if i==0 else {'body_id':'generic.proxy','endorphin_model':'physics/endorphin/humanoid.json'}
  scene['entities'].append({'kind':'mesh','name':'generic.authored.'+str(i),'transform':{'position':[3*i,1.15,2],'rotation_degrees':[0,0,0],'scale':[1,1,1]},
   'mesh':{'asset':models[i]['model']},'collider':{'solid':False},'character':component,'lod':{'visible_distance':180,'stream_distance':200,'fade_range':10}})
 scene['entities'].append({'kind':'entity','name':'generic.proxy','transform':{'position':[3,.95,2],'rotation_degrees':[0,0,0],'scale':[1,1,1]}})
 save(fixture/manifest['files']['scene'],scene)
 overlay=fixture/'assets/scripts/newviso';overlay.mkdir(parents=True,exist_ok=True)
 shutil.copyfile(WORKSPACE/'Shared/Content/scripts/newviso/default_game.ysc',overlay/'default_game_base.ysc')
 shutil.copyfile(ROOT/'scripts/fixtures/entity_stairs_verify.ysc',overlay/'default_game.ysc')
 shutil.copyfile(ROOT/'scripts/fixtures/foot_motion_audit.ysc',overlay/'foot_motion_audit.ysc')
 env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
 exe=ROOT/'target'/args.profile/'newviso.exe';digest=hashlib.sha256(exe.read_bytes()).hexdigest();start=time.time();log=cache/'native.log'
 with log.open('wb')as out:
  p=subprocess.Popen([str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','18000'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
  (cache/'native.pid').write_text(str(p.pid));captured=set()
  while p.poll()is None:
   if time.time()-start>180:p.terminate();p.wait(timeout=10);break
   text=log.read_text(encoding='utf-8',errors='replace')
   for label in ['middle','upper']+['gait'+str(i)for i in range(5)]:
    if label not in captured and 'ENTITY_STAIR_CAPTURE '+label in text:
     captured.add(label)
     subprocess.run(['C:/msys64/mingw64/bin/python.exe',str(ROOT/'scripts/capture_game_window.py'),str(cache),'stairs-'+label+'.png','NorthStar entity stairs verification'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
   time.sleep(.1)
 text=log.read_text(encoding='utf-8',errors='replace')
 samples=[json.loads(l.split('ENTITY_STAIR_SAMPLE ',1)[1])for l in text.splitlines()if 'ENTITY_STAIR_SAMPLE 'in l]
 result=[json.loads(l.split('ENTITY_STAIRS_VERIFY_PASS ',1)[1])for l in text.splitlines()if 'ENTITY_STAIRS_VERIFY_PASS 'in l]
 report={'passed':p.returncode==0 and bool(result) and not any(x in text for x in ['runtime application failed:','panicked at','TYPESCRIPT_SCRIPTING_COMPILE_FAILED']),'exit_code':p.returncode,'profile':args.profile,'executable_sha256':digest,'seconds':time.time()-start,'sample_count':len(samples),'result':result}
 save(cache/'samples.json',samples);save(cache/'acceptance.json',report);print(json.dumps(report))
 if not report['passed']:raise RuntimeError(text[-10000:])
if __name__=='__main__':main()

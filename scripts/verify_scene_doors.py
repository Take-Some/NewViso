"""Native, original-map test of engine-owned hinges on existing scene meshes."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,sys,time,re
ROOT=Path(__file__).resolve().parents[1];WORKSPACE=ROOT.parent
def save(p,value):
 p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode())
def main():
 a=argparse.ArgumentParser();a.add_argument('--project',default='FirstFPS');a.add_argument('--door',default='v_ilev_fb_door01')
 a.add_argument('--side',type=int,choices=[-1,1],default=1);a.add_argument('--capture',action='store_true');a.add_argument('--run-tag',default='');args=a.parse_args()
 if not re.fullmatch(r'[A-Za-z0-9_-]+',args.project+args.door+(args.run_tag or 'proof')):raise ValueError('unsafe proof label')
 source=WORKSPACE/'Projects'/args.project
 cache=ROOT/'.newviso/cache'/('scene-hinges-'+args.project.lower()+'-'+args.door+'-'+str(args.side)+('-'+args.run_tag if args.run_tag else '')+'-proof');fixture=cache/'fixture'
 if fixture.exists():shutil.rmtree(fixture)
 fixture.mkdir(parents=True)
 manifest=json.loads((source/'project.json').read_bytes())
 for key in ['runtime','environment','scene']:
  rel=manifest['files'][key];d=fixture/rel;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/rel,d)
 for path in [manifest['paths']['assets'],manifest['paths'].get('content','content')]:
  if (source/path).is_dir():shutil.copytree(source/path,fixture/path,copy_function=os.link,dirs_exist_ok=True)
 shutil.copytree(source/'scripts',fixture/'assets/scripts',copy_function=os.link,dirs_exist_ok=True)
 manifest['project']['id']='scene-hinge-verification';manifest['scripts']['entrypoint']='scripts/scene_doors_verify.ysc'
 save(fixture/'project.json',manifest)
 runtime=json.loads((fixture/manifest['files']['runtime']).read_bytes())
 runtime['world_persistence']={'load_on_start':False,'save_on_shutdown':False,'autosave_interval_seconds':0}
 runtime['window']={**runtime.get('window',{}),'title':'NorthStar scene hinge verification','width':960,'height':600}
 runtime['variables']['scene_door_verify']={'door':args.door,'side':args.side,'capture':args.capture,'player_body':900000000}
 save(fixture/manifest['files']['runtime'],runtime)
 shutil.copyfile(ROOT/'scripts/fixtures/scene_doors_verify.ysc',fixture/'assets/scripts/scene_doors_verify.ysc')
 env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
  NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),
  NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
 exe=ROOT/'target/release/newviso.exe';started=time.time();captured=set()
 with (cache/'native.log').open('wb') as log:
  proc=subprocess.Popen([str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','15000'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
  (cache/'native.pid').write_text(str(proc.pid))
  while proc.poll() is None:
   if time.time()-started>240:proc.terminate();proc.wait(timeout=10);raise RuntimeError('scene hinge proof timeout')
   if args.capture:
    text=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
    for name in ['closed','open','restored']:
     if name not in captured and 'SCENE_HINGE_CAPTURE '+name in text:
      captured.add(name)
      capture=subprocess.run([sys.executable,str(ROOT/'scripts/capture_game_window.py'),str(cache),name+'.png','NorthStar scene hinge verification'],capture_output=True,text=True)
      save(cache/(name+'-capture.json'),{'exit_code':capture.returncode,'stdout':capture.stdout,'stderr':capture.stderr})
   time.sleep(.05)
 text=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
 passed=proc.returncode==0 and 'SCENE_HINGE_VERIFY_PASS' in text and 'runtime application failed:' not in text
 if args.capture:passed=passed and all((cache/(x+'.png')).exists() for x in ['closed','open'])
 report={'passed':passed,'exit_code':proc.returncode,'project':args.project,'door':args.door,'side':args.side,'seconds':round(time.time()-started,2),'executable_sha256':hashlib.sha256(exe.read_bytes()).hexdigest()}
 for line in text.splitlines():
  for marker,key in [('SCENE_HINGE_VERIFY_EVIDENCE ','evidence'),('SCENE_HINGE_GEOMETRY ','geometry')]:
   if marker in line:report[key]=json.JSONDecoder().raw_decode(line.split(marker,1)[1])[0]
 errors=[l for l in text.splitlines() if 'runtime application failed:' in l]
 if errors:report['failure']=errors[0][:14000]
 save(cache/'acceptance.json',report);print(json.dumps(report))
 if not passed:raise RuntimeError(report.get('failure','scene proof did not pass'))
if __name__=='__main__':main()

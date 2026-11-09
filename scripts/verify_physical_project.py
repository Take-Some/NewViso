"""Native protective hands and authored door through VehicleTest's real controller."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,time
ROOT=Path(__file__).resolve().parents[1]
WORKSPACE=ROOT.parent
def main():
    source=WORKSPACE/'Projects/VehicleTest'
    cache=ROOT/'.newviso/cache/physical-project-proof';fixture=cache/'fixture'
    if fixture.exists():shutil.rmtree(fixture)
    fixture.mkdir(parents=True)
    manifest=json.loads((source/'project.json').read_bytes())
    for key in ['runtime','environment','scene']:
        rel=manifest['files'][key];dest=fixture/rel;dest.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(source/rel,dest)
    shutil.copytree(source/'assets',fixture/'assets',dirs_exist_ok=True)
    shutil.copytree(source/'scripts',fixture/'assets/scripts',dirs_exist_ok=True)
    manifest['project']['id']='physical-project-verification'
    manifest['scripts']['entrypoint']='scripts/physical_project_verify.ysc'
    (fixture/'project.json').write_bytes(json.dumps(manifest).encode())
    shutil.copyfile(ROOT/'scripts/fixtures/physical_project_verify.ysc',fixture/'assets/scripts/physical_project_verify.ysc')
    env=os.environ.copy();env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'),NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
        NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'),NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'),
        VULKAN_SDK=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk'),
        NEWENGINE_GLSLANG_VALIDATOR=str(WORKSPACE/'pluginsRuntime/toolchains/vulkan-sdk/Bin/glslangValidator.exe'))
    exe=ROOT/'target/release/newviso.exe';started=time.time()
    with (cache/'native.log').open('wb') as log:
        p=subprocess.Popen([str(exe),'--base-dir',str(ROOT),'--project',str(fixture),'--max-frames','12000'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
        (cache/'native.pid').write_text(str(p.pid))
        try:code=p.wait(timeout=200)
        except subprocess.TimeoutExpired:p.terminate();p.wait(timeout=10);raise RuntimeError('native project timeout')
    log=(cache/'native.log').read_text(encoding='utf-8',errors='replace')
    passed=code==0 and 'PHYSICAL_PROJECT_VERIFY_PASS' in log and 'runtime application failed:' not in log
    report={'passed':passed,'exit_code':code,'seconds':round(time.time()-started,2),'project':'VehicleTest',
        'checks':['original_project_controller','actual_player_proxy','protective_hands','authored_door_physical_push'],
        'executable_sha256':hashlib.sha256(exe.read_bytes()).hexdigest()}
    for line in log.splitlines():
        if 'PHYSICAL_PROJECT_VERIFY_EVIDENCE ' in line:report['evidence']=json.JSONDecoder().raw_decode(line.split('PHYSICAL_PROJECT_VERIFY_EVIDENCE ',1)[1])[0]
    errors=[l for l in log.splitlines() if 'runtime application failed:' in l]
    if errors:report['failure']=errors[0][:9000]
    (cache/'acceptance.json').write_bytes(json.dumps(report,indent=2).encode())
    print(json.dumps(report))
    if not passed:raise RuntimeError(report.get('failure','project proof did not pass'))
if __name__=='__main__':main()

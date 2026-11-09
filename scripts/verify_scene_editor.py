"""Exercise shared scene contracts, transactional disk edits and native deployment."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, sys, time

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent
CACHE = ROOT / '.newviso/cache/editor-proof'

def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--build-profile', choices=['debug', 'release'], default='release')
    parser.add_argument('--native', action='store_true')
    parser.add_argument('--package-output', type=Path)
    args = parser.parse_args()
    CACHE.mkdir(parents=True, exist_ok=True)
    tool = ROOT / 'target' / args.build_profile / 'newviso-project-tool.exe'
    engine = ROOT / 'target' / args.build_profile / 'newviso.exe'
    env = os.environ.copy()
    env.update(NEWVISO_ASSETS_DIR=str(WORKSPACE/'Shared/Content'), NEWVISO_CONTENT_DIR=str(WORKSPACE/'Shared/Content'),
               NEWVISO_PROVIDER_DIR=str(WORKSPACE/'pluginsRuntime'), NEWVISO_CODECS_DIR=str(WORKSPACE/'pluginsRuntime/codecs'))
    env.pop('NEWVISO_RUNTIME_PROFILE', None)
    def run(command, project=None, extra=(), success=True, timeout=30):
        cmd = [str(tool), command, '--base-dir', str(ROOT)]
        if project: cmd += ['--project', str(project)]
        proc = subprocess.run(cmd+list(map(str, extra)), cwd=ROOT, env=env, capture_output=True, encoding='utf-8', timeout=timeout)
        if not success:
            assert proc.returncode != 0, f'invalid request accepted: {extra}'
            return {'rejected': True, 'error': proc.stderr.strip()}
        assert proc.returncode == 0, proc.stderr
        return json.loads(proc.stdout)
    report = {'passed': False, 'build_profile': args.build_profile}
    registry = run('types')
    assert len(registry['components']) >= 6 and 'cube' in registry['types']
    authored=json.loads((ROOT/'crates/newviso-scene-document/src/assets/scene-types.json').read_text(encoding='utf-8'))
    assert registry['components'].keys()==authored['components'].keys(), 'tool registry is stale'
    for id,component in authored['components'].items():
        assert all(registry['components'][id].get(k)==value for k,value in component.items() if k!='properties'), f'stale component metadata: {id}'
        expected={p['path']:p for p in component['properties']}
        actual={p['path']:p for p in registry['components'][id]['properties']}
        assert expected.keys()==actual.keys(), f'stale component properties: {id}'
        for path,prop in expected.items():
            assert all(actual[path][k]==value for k,value in prop.items()), f'stale property metadata: {path}'
    report['registry'] = {'components': len(registry['components']), 'types': len(registry['types'])}
    report['scenes'] = []
    for project in sorted((WORKSPACE/'Projects').iterdir()):
        if (project/'project.json').is_file():
            report['scenes'].append({'project': project.name, **run('scene-validate', project)})
    # Only the disposable fixture is changed by edit tests.
    fixture = CACHE / 'authoring-fixture'
    if fixture.exists(): shutil.rmtree(fixture)
    shutil.copytree(WORKSPACE/'Projects/WeaponLab', fixture,
                    ignore=shutil.ignore_patterns('.newviso', '.git', 'target', 'tools', '*.bak-*'))
    manifest = json.loads((fixture/'project.json').read_text(encoding='utf-8'))
    scene = fixture/manifest['files']['scene']
    inspected = run('scene-inspect', fixture, ['--entity', '1'])
    request = CACHE/'edits.json'
    save(request, {'expected_blake3': inspected['blake3'], 'edits': [
        {'op':'set','entity':1,'path':'/transform/position','value':[2,3,-4]},
        {'op':'set','entity':1,'path':'/name','value':'Authoring proof'}]})
    edited = run('scene-edit', fixture, ['--edits',request])
    assert edited['passed']
    current = json.loads(scene.read_text(encoding='utf-8'))
    assert current['entities'][1]['transform']['position'] == [2,3,-4]
    report['stale_revision'] = run('scene-edit',fixture,['--edits',request],False)
    inspected = run('scene-inspect',fixture)
    before = scene.read_bytes()
    save(request, {'expected_blake3':inspected['blake3'], 'edits':[
        {'op':'set','entity':1,'path':'/name','value':'Must not commit'},
        {'op':'set','entity':0,'path':'/camera/near','value':999999}]})
    report['invalid_batch'] = run('scene-edit',fixture,['--edits',request],False)
    assert scene.read_bytes() == before
    report['transactional_disk_edit'] = True
    if args.native:
        title = 'NorthStar scene editor verification'
        cmd = [str(engine),'--base-dir',str(ROOT),'--project',str(fixture),'--editor','--max-frames','240',
               '--runtime-set',f'window.title={title}','--runtime-set','window.width=1280','--runtime-set','window.height=800']
        started=time.time()
        with (CACHE/'native-editor.log').open('wb') as log:
            proc=subprocess.Popen(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
            (CACHE/'native-editor.pid').write_text(str(proc.pid))
            try: code=proc.wait(timeout=150)
            except subprocess.TimeoutExpired:
                proc.terminate();proc.wait(timeout=10);raise RuntimeError('native editor timed out')
        log=(CACHE/'native-editor.log').read_text(encoding='utf-8',errors='replace')
        assert code==0 and 'project content manager activated' in log and 'project scripting suppressed by authoring/safe mode' in log, log[-3000:]
        assert 'runtime application failed:' not in log and 'panicked at' not in log,log[-3000:]
        assert 'first UI draw list screen=' in log, 'UI provider did not produce a draw frame'
        assert "UI composite ready domain=editor executed_passes=1 skipped_passes=0" in log, 'renderer did not execute the editor composite pass'
        mesh_line=next(line for line in log.splitlines() if 'first UI draw list screen=' in line)
        assert ' vertices=0 ' not in mesh_line, mesh_line
        report['native_editor']={'passed':True,'exit_code':code,'seconds':round(time.time()-started,2)}
    if args.package_output:
        project=WORKSPACE/'Projects/WeaponLab'
        output=args.package_output.resolve()
        report['package_plan']=run('package-plan',project)
        report['package']=run('package-build',project,['--output',output],timeout=300)
        report['package_integrity']=run('package-verify',extra=['--package-dir',output],timeout=120)
        # A relocated, independently mounted copy must boot with the packaged resources.
        isolated=os.environ.copy()
        for key in list(isolated):
            if key.startswith(('NEWVISO_','NEWENGINE_')) or key in ['CACHE_FILES','VULKAN_SDK']:
                isolated.pop(key,None)
        isolated['VULKAN_SDK']=str(output/'providers/toolchains/vulkan-sdk')
        isolated['NEWENGINE_GLSLANG_VALIDATOR']=str(output/'providers/toolchains/vulkan-sdk/Bin/glslangValidator.exe')
        cmd=['cmd.exe','/d','/c',str(output/'Launch.cmd'),'--max-frames','60']
        with (CACHE/'native-package.log').open('wb') as log:
            proc=subprocess.Popen(cmd,cwd=CACHE,env=isolated,stdout=log,stderr=subprocess.STDOUT)
            try:code=proc.wait(timeout=150)
            except subprocess.TimeoutExpired:
                proc.terminate();proc.wait(timeout=10);raise RuntimeError('native package timed out')
        log=(CACHE/'native-package.log').read_text(encoding='utf-8',errors='replace')
        assert code==0 and 'platform stopped frames=60' in log,log[-4000:]
        assert not any(x in log for x in ['runtime application failed:','TYPESCRIPT_SCRIPTING_COMPILE_FAILED','panicked at']),log[-4000:]
        assert str(output/'shared').lower() in log.lower(), 'packaged shared mount was not used'
        report['native_package']={'passed':True,'exit_code':code,'frames':60}
        report['package_integrity_after_launch']=run('package-verify',extra=['--package-dir',output],timeout=120)
    baseline=ROOT/'.newviso/cache/engine-configuration-gameplay-baseline.json'
    if baseline.exists():
        hashes=json.loads(baseline.read_text(encoding='utf-8'))
        assert all(hashlib.sha256((WORKSPACE/p).read_bytes()).hexdigest()==sha for p,sha in hashes.items())
        report['unchanged_gameplay_modules']=len(hashes)
    report['passed']=True
    save(CACHE/'verification.json',report)
    print(json.dumps(report,ensure_ascii=False))

if __name__ == '__main__':
    try:main()
    except Exception as error:
        save(CACHE/'verification.json',{'passed':False,'error':str(error)})
        raise

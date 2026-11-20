from pathlib import Path
import subprocess,time
root=Path(__file__).resolve().parents[2]
game=root/'Projects/KaylaNightCity'
cli=root/'Tools/third_party/WolvenKitCLI'
source=game/'.newviso/cache/ink_full_ui_original/base/gameplay/gui/widgets/healthbar/playerhealthbar.inkwidget'
out=game/'.newviso/cache/ink_full_ui_decoded/hud_widgets'
out.mkdir(parents=True,exist_ok=True)
t=time.monotonic()
r=subprocess.run([str(cli/'dotnet10/dotnet.exe'),str(cli/'10.0.0-beta.1/WolvenKit.CLI.dll'),'convert','serialize',str(source),'-o',str(out)],capture_output=True,text=True,errors='replace',timeout=60)
print('returncode',r.returncode,'duration',time.monotonic()-t)
print('stdout',r.stdout[-1600:],'stderr',r.stderr[-1600:])
print('outputs',[(f.name,f.stat().st_size) for f in out.iterdir() if f.is_file()][:12])

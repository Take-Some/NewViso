#!/usr/bin/env python3
"""Cook the original water recordings into a project-independent Shared YSCD.

AWC decoding follows the supplied RAGE container/waveslot definitions. Keys are
read from that source tree in memory; they are never included in the output.
The REL provenance records distinguish PCM leaves from unported synth nodes.
"""
from pathlib import Path
import argparse, hashlib, json, re, shutil, struct, subprocess, wave

def joaat(text):
    h = 0
    for c in text.lower().encode("utf-8"):
        h = (h + c) & 0xffffffff
        h = (h + (h << 10)) & 0xffffffff
        h ^= h >> 6
    h = (h + (h << 3)) & 0xffffffff
    h ^= h >> 11
    return (h + (h << 15)) & 0xffffffff

def read_rel(path):
    data = path.read_bytes()
    assert struct.unpack_from("<I", data)[0] == 54, path
    nt = 8 + struct.unpack_from("<I", data, 4)[0]
    size, count = struct.unpack_from("<II", data, nt)
    strings = nt + 8 + 4 * count
    names = [data[strings + struct.unpack_from("<I", data, nt + 8 + 4*i)[0]:]
             .split(b"\0", 1)[0].decode("ascii") for i in range(count)]
    index = nt + size + 4
    records = {}
    for i in range(struct.unpack_from("<I", data, index)[0]):
        name, offset, length = struct.unpack_from("<III", data, index + 4 + 12*i)
        assert 8 + offset + length <= nt
        records[name] = data[8 + offset:8 + offset + length]
    return records, names

def closure(records, root):
    pending, seen, leaves, synths = [root], set(), [], []
    while pending:
        name = pending.pop()
        if name in seen or name not in records:
            continue
        seen.add(name)
        data = records[name]
        if data[0] == 12:
            bank, sample = struct.unpack_from("<II", data, len(data)-9)
            leaves.append((name, bank, sample))
            continue
        if data[0] == 18:
            synths.append(name)
        # Metadata references are hashes, with packed, variable-size headers.
        # Resolve candidates only against the REL's authoritative object index.
        pending.extend({struct.unpack_from("<I", data, i)[0]
                        for i in range(5, len(data)-3)
                        if struct.unpack_from("<I", data, i)[0] in records})
    return sorted(leaves), sorted(synths)

def run(argv, **kwargs):
    result = subprocess.run([str(a) for a in argv], capture_output=True,
                            text=True, errors="replace", timeout=120, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{Path(argv[0]).name} failed ({result.returncode}): "
                           f"{result.stderr[-1000:]}")
    return result.stdout

def cue(name, clips, looping=False, spatial=True, gain=1.0):
    return {
        "name":name, "route":"sfx", "looping":looping,
        "concurrency_group":"water."+name,
        "concurrency_limit":2 if looping else 6, "concurrency_scope":"global",
        "steal_rule":"lower_priority_then_oldest", "voice_budget":"gameplay",
        "priority":50 if looping else 62, "repeat_avoidance":min(2,max(0,len(clips)-1)),
        "spatial_policy":"spatial" if spatial else "non_spatial",
        "gain_range":[gain,gain], "pitch_range":[1.0,1.0],
        "attenuation":{"min_distance":3.0,"max_distance":100.0,
                       "curve":"inverse","rolloff":0.8,"curve_points":[]},
        "layers":[],
        "clips":[{"name":Path(p).stem,"source":p,"weight":1.0,"gain":1.0,"pitch":1.0}
                 for p in clips]
    }

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--game", type=Path, required=True)
    parser.add_argument("--source-code", type=Path, required=True,
                        help="GTAV Source/src/dev_ng directory")
    args = parser.parse_args()
    root = args.workspace
    work = root/"Tools/tmp/water-work/audio-reference"
    source = root/"Shared/Source/audio/environment/water"
    content = root/"Shared/Content/audio/environment/water"
    for p in [work,source/"reference",source/"recordings",content]:
        p.mkdir(parents=True, exist_ok=True)
    rsc = root/"Tools/rsc7-cli/bin/rsc7.exe"
    decoder = root/"Tools/toolbelt/third_party/vgmstream/bin/vgmstream-cli.exe"
    packer = root/"Tools/toolbelt/first_party/northstar/yscd_packer/bin/northstar-yscd-packer.exe"
    def extract(archive, member):
        output = work/"extracted"
        output.mkdir(exist_ok=True)
        found = next(iter(output.rglob(Path(member).name)),None)
        if not found:
            run([rsc,"--no-update-check","--keys",root/"Tools/rsc7-cli/keys",
                 "extract",args.game/archive,member,"-o",output])
            found = next(output.rglob(Path(member).name))
        return found
    records, bank_names = read_rel(extract("x64/audio/audio_rel.rpf","config/sounds.dat54.rel"))
    sp, sp_names = read_rel(extract("x64/audio/audio_rel.rpf","config/sp_sounds.dat54.rel"))
    records.update(sp);bank_names += sp_names
    banks = {joaat(n.replace("\\","/")):n for n in bank_names}
    set_record = records[0xb12515a5]
    assert set_record[0] == 32
    phase_hashes = dict(struct.unpack_from("<II",set_record,9+i*8)
                        for i in range(struct.unpack_from("<I",set_record,5)[0]))
    selected = {"ocean":phase_hashes[0xbae52a34],
                "wave_break":phase_hashes[0x4eaa94cf],
                "wave_end":phase_hashes[0xd6608543]}
    if 0xb44ec0a2 in records:
        selected["splash"] = 0xb44ec0a2
    if 0xcf6e6d23 in records:
        selected["exit"] = 0xcf6e6d23
    # Preserve the original controls and synth references for further DSP ports.
    phases = {}
    for name, h in selected.items():
        leaves,synths = closure(records,h)
        phases[name] = {"sound_hash":f"{h:08x}", "pcm":[
            {"sound_hash":f"{s:08x}","bank_hash":f"{b:08x}",
             "bank":banks.get(b),"wave_hash":f"{w:08x}"} for s,b,w in leaves],
            "synth_nodes":[f"{s:08x}" for s in synths]}
    cpp = Path(__file__).with_name("water_awc_decode.cpp")
    executable = work/"awc-decrypt.exe"
    if not executable.exists() or executable.stat().st_mtime < cpp.stat().st_mtime:
        run(["C:/msys64/mingw64/bin/g++.exe","-O2","-std=c++17",cpp,"-o",executable])
    keys_text = (args.source_code/"rage/base/src/audiohardware/waveslot.cpp").read_text(errors="replace")
    arrays = re.findall(r"static const u32 key\[\]\s*=\s*\{([^}]+)\}",keys_text)
    keys = re.findall(r"0x[0-9a-fA-F]+",arrays[-1])
    assert len(keys) == 4, "PC key layout changed"
    bank_cache, recording_info = {}, {}
    def recording(bank_hash, wave_hash):
        bank_name = banks[bank_hash].replace("\\","/")
        if bank_name not in bank_cache:
            folder, filename = bank_name.split("/",1)
            raw = extract("x64/audio/sfx/"+folder+".rpf",filename.lower()+".awc")
            shutil.copy2(raw,source/"reference"/(filename.lower()+".awc"))
            decoded = work/(filename.lower()+".decoded.awc")
            run([executable,raw,decoded],input=" ".join(keys))
            data = decoded.read_bytes()
            version,count = struct.unpack_from("<II",data,4)
            offset = 16+(count*2 if version&0x10000 else 0)
            ids = {struct.unpack_from("<I",data,offset+i*4)[0]&0x1fffffff:i+1
                   for i in range(count)}
            bank_cache[bank_name] = decoded,ids
        decoded, ids = bank_cache[bank_name]
        stream = ids[wave_hash&0x1fffffff]
        relative = "recordings/"+bank_name.split("/")[-1].lower()+f"_{wave_hash&0x1fffffff:08x}.wav"
        out = source/relative
        if not out.exists():
            run([decoder,"-i","-s",stream,"-o",out,decoded])
        with wave.open(str(out),"rb") as w:
            assert w.getnchannels() in [1,2]
            assert w.getsampwidth() == 2
            recording_info[relative] = {"bank":bank_name,"wave_hash":f"{wave_hash:08x}",
                "subsong":stream,"sample_rate":w.getframerate(),"channels":w.getnchannels(),
                "frames":w.getnframes(),"sha256":hashlib.sha256(out.read_bytes()).hexdigest()}
        return relative
    cues=[]
    for name,h in selected.items():
        leaves,_=closure(records,h)
        clips=sorted({recording(b,w) for s,b,w in leaves})
        if clips:
            cues.append(cue(name,clips,looping=name=="ocean",spatial=name!="ocean",
                            gain=0.55 if name=="ocean" else 0.75))
    underwater = extract("x64/audio/sfx/STREAMS.rpf","underwater_8ch.awc")
    shutil.copy2(underwater,source/"reference/underwater_8ch.awc")
    underwater_rel="recordings/underwater_stereo.wav"
    underwater_out=source/underwater_rel
    if not underwater_out.exists():
        # Decoder preserves the original 48 kHz timebase and mixes all 8 channels.
        run([decoder,"-i","-D","2","-o",underwater_out,underwater])
    with wave.open(str(underwater_out),"rb") as w:
        assert w.getnchannels()==2 and w.getframerate()==48000
        recording_info[underwater_rel]={"bank":"STREAMS/UNDERWATER_8CH",
            "mix":"vgmstream -D 2, original 8 channels to stereo",
            "sample_rate":w.getframerate(),"channels":2,"frames":w.getnframes(),
            "sha256":hashlib.sha256(underwater_out.read_bytes()).hexdigest()}
    cues.append(cue("underwater",[underwater_rel],looping=True,spatial=False,gain=0.6))
    manifest = source/"water.yscd.json"
    manifest.write_text(json.dumps({"schema":"newengine.yscd.manifest.v1","version":1,
                                   "cues":cues},indent=2)+"\n",encoding="utf-8")
    output=content/"water.yscd"
    run([packer,"pack","--input",manifest,"--output",output])
    run([packer,"verify","--input",output])
    provenance={"schema":"northstar.water.audio.import.v1",
        "source":"original AWC/REL recordings","soundset":"OCEAN_BEACH_SOUNDSET",
        "soundset_hash":"b12515a5","phases":phases,"recordings":recording_info,
        "unported":"ModularSynthSound DSP and original dynamic mix graph",
        "dictionary_sha256":hashlib.sha256(output.read_bytes()).hexdigest(),
        "cue_count":len(cues),"recording_count":len(recording_info)}
    (source/"import_manifest.json").write_text(json.dumps(provenance,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"dictionary":str(output),"cues":[c["name"] for c in cues],
                      "recordings":len(recording_info),"bytes":output.stat().st_size}))
if __name__=="__main__":
    main()

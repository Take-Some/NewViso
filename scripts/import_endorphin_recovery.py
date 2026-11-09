"""Inspect a GTA V pose matcher and prepare an explicit NewViso recovery profile."""

from argparse import ArgumentParser
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import struct
import xml.etree.ElementTree as ET


def read_pose_database(path):
    data = path.read_bytes()
    offset = 0

    def read(fmt):
        nonlocal offset
        size = struct.calcsize(fmt)
        if offset + size > len(data):
            raise ValueError(f"truncated pose database at byte {offset}")
        values = struct.unpack_from(fmt, data, offset)
        offset += size
        return values

    marker, stream_version, version, signature, count = read("<BIIII")
    if (marker, stream_version, version) != (26, 2, 0) or not 1 <= count <= 32768:
        raise ValueError("unsupported pose database header")

    times = defaultdict(list)
    cloud_weights_reference = None
    for _ in range(count):
        set_hash, clip_hash, time, points = read("<IIfI")
        if not math.isfinite(time) or time < 0 or not 1 <= points <= 512:
            raise ValueError("invalid pose sample time or point count")
        positions = read(f"<{3 * points}f")
        cloud_weights, = read("<I")
        if cloud_weights != points:
            raise ValueError("pose sample point/weight count mismatch")
        weights = read(f"<{cloud_weights}f")
        bounds = read("<7f")
        if not all(math.isfinite(value) for value in (*positions, *weights, *bounds)):
            raise ValueError("non-finite pose sample")
        if cloud_weights_reference is None:
            cloud_weights_reference = weights
        elif weights != cloud_weights_reference:
            raise ValueError("pose sample weights vary within the database")
        times[(set_hash, clip_hash)].append(time)

    bone_count, = read("<I")
    if not 1 <= bone_count <= 512:
        raise ValueError("invalid pose database bone count")
    bones = read(f"<{bone_count}H")
    weight_count, = read("<I")
    if weight_count != bone_count:
        raise ValueError("pose database bone/weight count mismatch")
    weights = read(f"<{weight_count}f")
    interval, window, repeated_signature = read("<fII")
    if (offset != len(data) or signature != repeated_signature
            or not math.isfinite(interval) or interval <= 0 or window != 1
            or any(not math.isfinite(weight) or weight < 0 for weight in weights)):
        raise ValueError("unsupported pose database footer or history window")

    active = [(tag, weight) for tag, weight in zip(bones, weights) if weight > 0]
    active_weights = tuple(weight for _, weight in active)
    if len(active) < 3 or len({tag for tag, _ in active}) != len(active):
        raise ValueError("invalid active bone set")
    if cloud_weights_reference != active_weights:
        raise ValueError("pose cloud weights differ from the database joint weights")
    return {
        "sha256": hashlib.sha256(data).hexdigest(),
        "skeleton_signature": signature,
        "history_interval_seconds": interval,
        "sample_count": count,
        "joint_weights": active,
        "sample_times": dict(times),
    }


def bind_joint_weights(database, skeleton):
    names = {}
    for joint in ET.parse(skeleton).getroot().findall("./Joint"):
        tag = int(joint.attrib["tag"], 0)
        name = joint.attrib["name"]
        if tag in names and names[tag] != name:
            raise ValueError(f"ambiguous skeleton bone tag {tag:#06x}")
        names[tag] = name
    joints = []
    for tag, weight in database["joint_weights"]:
        if tag not in names:
            raise ValueError(f"missing weighted bone {tag:#06x} in {skeleton}")
        joints.append({"joint": names[tag], "weight": weight})
    return joints


def hash_name(name):
    value = 0
    for byte in name.lower().encode("ascii"):
        value = (value + byte) & 0xffffffff
        value = (value + (value << 10)) & 0xffffffff
        value ^= value >> 6
    value = (value + (value << 3)) & 0xffffffff
    value ^= value >> 11
    return (value + (value << 15)) & 0xffffffff


def build_profile(database, skeleton, metadata, bindings):
    sets = ET.parse(metadata).getroot().findall("./sets/Item")
    clips = {}
    for binding in bindings:
        runtime, set_key, clip_set, clip = (
            binding[key] for key in ("runtime_clip", "set", "clip_set", "clip")
        )
        if runtime in clips or not all(runtime.rpartition("@")[::2]):
            raise ValueError(f"duplicate or invalid runtime clip {runtime}")
        items = [
            item for group in sets if group.get("key") == set_key
            for item in group.findall("./items/Item")
            if (item.findtext("clipSet") or "").lower() == clip_set.lower()
            and (item.findtext("clip") or "").lower() == clip.lower()
            and item.findtext("type") == "NMBT_SINGLECLIP"
            and item.find("./addToPointCloud[@value='true']") is not None
        ]
        key = (hash_name(clip_set), hash_name(clip))
        if len(items) != 1 or key not in database["sample_times"]:
            raise ValueError(f"no unique single-clip sample and transition for {set_key}/{clip_set}/{clip}")
        blend = float(items[0].find("./ragdollFrameBlendDuration").attrib["value"])
        if not math.isfinite(blend) or blend <= 0:
            raise ValueError(f"invalid blend duration for {clip_set}/{clip}")
        clips[runtime] = {
            "sample_times_seconds": sorted(set(database["sample_times"][key])),
            "blend_seconds": blend,
        }
    if not clips:
        raise ValueError("at least one exact runtime clip binding is required")
    return {"joints": bind_joint_weights(database, skeleton), "clips": clips}


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("--skeleton", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--bindings", type=Path)
    args = parser.parse_args()
    database = read_pose_database(args.database)
    if args.bindings:
        if not args.skeleton or not args.metadata:
            parser.error("--bindings requires --skeleton and --metadata")
        result = build_profile(database, args.skeleton, args.metadata,
                               json.loads(args.bindings.read_text(encoding="utf-8")))
    else:
        result = {k: v for k, v in database.items() if k != "sample_times"}
        result["sample_times"] = {
            f"{set_hash:08x}/{clip_hash:08x}": times
            for (set_hash, clip_hash), times in database["sample_times"].items()
        }
        if args.skeleton:
            result["joints"] = bind_joint_weights(database, args.skeleton)
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()

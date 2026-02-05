#!/usr/bin/env python3
"""
Remaps animations from Universal Animation Library v2 bone names
to the v1 DEF-* bone naming convention used by the player model.
Handles: Slide, Hook, Melee animations.
"""
import json
import struct
import sys
from pathlib import Path

# v2 bone name -> v1 bone name mapping
BONE_MAP = {
    "pelvis": "DEF-hips",
    "spine_01": "DEF-spine001",
    "spine_02": "DEF-spine002",
    "spine_03": "DEF-spine003",
    "neck_01": "DEF-neck",
    "Head": "DEF-head",
    "clavicle_l": "DEF-shoulderL",
    "clavicle_r": "DEF-shoulderR",
    "upperarm_l": "DEF-upper_armL",
    "upperarm_r": "DEF-upper_armR",
    "lowerarm_l": "DEF-forearmL",
    "lowerarm_r": "DEF-forearmR",
    "hand_l": "DEF-handL",
    "hand_r": "DEF-handR",
    "thigh_l": "DEF-thighL",
    "thigh_r": "DEF-thighR",
    "calf_l": "DEF-shinL",
    "calf_r": "DEF-shinR",
    "foot_l": "DEF-footL",
    "foot_r": "DEF-footR",
    "ball_l": "DEF-toeL",
    "ball_r": "DEF-toeR",
    # Fingers - left
    "thumb_01_l": "DEF-thumb01L",
    "thumb_02_l": "DEF-thumb02L",
    "thumb_03_l": "DEF-thumb03L",
    "index_01_l": "DEF-f_index01L",
    "index_02_l": "DEF-f_index02L",
    "index_03_l": "DEF-f_index03L",
    "middle_01_l": "DEF-f_middle01L",
    "middle_02_l": "DEF-f_middle02L",
    "middle_03_l": "DEF-f_middle03L",
    "ring_01_l": "DEF-f_ring01L",
    "ring_02_l": "DEF-f_ring02L",
    "ring_03_l": "DEF-f_ring03L",
    "pinky_01_l": "DEF-f_pinky01L",
    "pinky_02_l": "DEF-f_pinky02L",
    "pinky_03_l": "DEF-f_pinky03L",
    # Fingers - right
    "thumb_01_r": "DEF-thumb01R",
    "thumb_02_r": "DEF-thumb02R",
    "thumb_03_r": "DEF-thumb03R",
    "index_01_r": "DEF-f_index01R",
    "index_02_r": "DEF-f_index02R",
    "index_03_r": "DEF-f_index03R",
    "middle_01_r": "DEF-f_middle01R",
    "middle_02_r": "DEF-f_middle02R",
    "middle_03_r": "DEF-f_middle03R",
    "ring_01_r": "DEF-f_ring01R",
    "ring_02_r": "DEF-f_ring02R",
    "ring_03_r": "DEF-f_ring03R",
    "pinky_01_r": "DEF-f_pinky01R",
    "pinky_02_r": "DEF-f_pinky02R",
    "pinky_03_r": "DEF-f_pinky03R",
    # Root stays the same
    "root": "root",
}


def read_glb(path: Path) -> tuple[dict, bytes]:
    """Read GLB file, return (json_data, binary_chunk)"""
    with open(path, 'rb') as f:
        # Header
        magic = f.read(4)
        if magic != b'glTF':
            raise ValueError("Not a GLB file")
        version = struct.unpack('<I', f.read(4))[0]
        length = struct.unpack('<I', f.read(4))[0]

        # JSON chunk
        json_length = struct.unpack('<I', f.read(4))[0]
        json_type = f.read(4)
        json_data = json.loads(f.read(json_length))

        # Binary chunk (if present)
        binary_chunk = b''
        if f.tell() < length:
            bin_length = struct.unpack('<I', f.read(4))[0]
            bin_type = f.read(4)
            binary_chunk = f.read(bin_length)

    return json_data, binary_chunk


def write_glb(path: Path, json_data: dict, binary_chunk: bytes):
    """Write GLB file"""
    json_bytes = json.dumps(json_data, separators=(',', ':')).encode('utf-8')
    # Pad JSON to 4-byte alignment
    while len(json_bytes) % 4 != 0:
        json_bytes += b' '

    # Pad binary to 4-byte alignment
    while len(binary_chunk) % 4 != 0:
        binary_chunk += b'\x00'

    total_length = 12 + 8 + len(json_bytes) + 8 + len(binary_chunk)

    with open(path, 'wb') as f:
        # Header
        f.write(b'glTF')
        f.write(struct.pack('<I', 2))  # version
        f.write(struct.pack('<I', total_length))

        # JSON chunk
        f.write(struct.pack('<I', len(json_bytes)))
        f.write(b'JSON')
        f.write(json_bytes)

        # Binary chunk
        f.write(struct.pack('<I', len(binary_chunk)))
        f.write(b'BIN\x00')
        f.write(binary_chunk)


def remap_animations(glb_path: Path):
    """Remap slide animations to use v1 bone names"""
    print(f"Reading {glb_path}...")
    json_data, binary_chunk = read_glb(glb_path)

    nodes = json_data.get('nodes', [])
    animations = json_data.get('animations', [])

    # Build node index lookup by name
    name_to_idx = {node.get('name'): i for i, node in enumerate(nodes)}

    # Build v2 name -> v1 index mapping
    v2_to_v1_idx = {}
    for v2_name, v1_name in BONE_MAP.items():
        if v2_name in name_to_idx and v1_name in name_to_idx:
            v2_to_v1_idx[name_to_idx[v2_name]] = name_to_idx[v1_name]

    print(f"Found {len(v2_to_v1_idx)} bone mappings")

    # Remap ALL animations that have v2 bone references
    remapped_count = 0
    for anim in animations:
        name = anim.get('name', '')

        # Check if this animation uses any v2 bones
        uses_v2_bones = False
        for channel in anim.get('channels', []):
            target = channel.get('target', {})
            node_idx = target.get('node')
            if node_idx in v2_to_v1_idx:
                uses_v2_bones = True
                break

        if not uses_v2_bones:
            continue

        print(f"Remapping animation: {name}")
        for channel in anim.get('channels', []):
            target = channel.get('target', {})
            node_idx = target.get('node')

            if node_idx in v2_to_v1_idx:
                old_name = nodes[node_idx].get('name')
                new_idx = v2_to_v1_idx[node_idx]
                new_name = nodes[new_idx].get('name')
                target['node'] = new_idx
                remapped_count += 1

    print(f"Remapped {remapped_count} animation channels")

    # Write output
    output_path = glb_path.parent / f"{glb_path.stem}_remapped{glb_path.suffix}"
    print(f"Writing {output_path}...")
    write_glb(output_path, json_data, binary_chunk)
    print("Done!")
    return output_path


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print("Usage: python remap_slide_animations.py <player.glb>")
        sys.exit(1)

    glb_path = Path(sys.argv[1])
    if not glb_path.exists():
        print(f"File not found: {glb_path}")
        sys.exit(1)

    output = remap_animations(glb_path)
    print(f"\nRemapped GLB written to: {output}")
    print("Replace your player.glb with this file to use the remapped animations.")

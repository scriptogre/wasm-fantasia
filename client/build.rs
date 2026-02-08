//! Generates `player.glb` from `player.source.glb` by stripping animations
//! not referenced in `Animation::clip_name()` and pruning unreferenced
//! binary data at accessor granularity. Runs automatically via cargo,
//! re-triggered only when the source files change.

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;

const ANIMATION_RS: &str = "src/player/animation.rs";
const SOURCE_GLB: &str = "assets/models/player.source.glb";
const OUTPUT_GLB: &str = "assets/models/player.glb";

const GLB_MAGIC: u32 = 0x46546C67;
const GLB_HEADER_SIZE: usize = 12;
const CHUNK_HEADER_SIZE: usize = 8;
const JSON_CHUNK_TYPE: u32 = 0x4E4F534A;
const BIN_CHUNK_TYPE: u32 = 0x004E4942;

/// Check if player.glb exists and is newer than both source files.
fn is_up_to_date() -> bool {
    let Ok(output_meta) = fs::metadata(OUTPUT_GLB) else {
        return false;
    };
    let Ok(output_mtime) = output_meta.modified() else {
        return false;
    };
    for input in [SOURCE_GLB, ANIMATION_RS] {
        let Ok(meta) = fs::metadata(input) else {
            return false;
        };
        if let Ok(mtime) = meta.modified() {
            if mtime > output_mtime {
                return false;
            }
        }
    }
    true
}

/// Byte size per component type (glTF spec)
fn component_byte_size(component_type: u64) -> usize {
    match component_type {
        5120 | 5121 => 1, // BYTE, UNSIGNED_BYTE
        5122 | 5123 => 2, // SHORT, UNSIGNED_SHORT
        5125 | 5126 => 4, // UNSIGNED_INT, FLOAT
        _ => 4,
    }
}

/// Element count per accessor type
fn type_element_count(accessor_type: &str) -> usize {
    match accessor_type {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => 1,
    }
}

fn main() {
    // Cargo re-runs this script only when these specific files change.
    println!("cargo:rerun-if-changed={SOURCE_GLB}");
    println!("cargo:rerun-if-changed={ANIMATION_RS}");

    // Skip if output already exists and is newer than both inputs.
    if is_up_to_date() {
        return;
    }

    let clip_names = parse_clip_names();
    let glb_data = fs::read(SOURCE_GLB).expect("failed to read player.source.glb");

    assert_eq!(
        u32::from_le_bytes(glb_data[0..4].try_into().unwrap()),
        GLB_MAGIC,
        "not a valid GLB file"
    );

    let json_len = u32::from_le_bytes(glb_data[12..16].try_into().unwrap()) as usize;
    assert_eq!(
        u32::from_le_bytes(glb_data[16..20].try_into().unwrap()),
        JSON_CHUNK_TYPE
    );
    let json_bytes = &glb_data[20..20 + json_len];

    let bin_chunk_offset = GLB_HEADER_SIZE + CHUNK_HEADER_SIZE + json_len;
    let bin_data = if bin_chunk_offset + CHUNK_HEADER_SIZE <= glb_data.len() {
        let bin_len =
            u32::from_le_bytes(glb_data[bin_chunk_offset..bin_chunk_offset + 4].try_into().unwrap())
                as usize;
        assert_eq!(
            u32::from_le_bytes(
                glb_data[bin_chunk_offset + 4..bin_chunk_offset + 8]
                    .try_into()
                    .unwrap()
            ),
            BIN_CHUNK_TYPE
        );
        &glb_data
            [bin_chunk_offset + CHUNK_HEADER_SIZE..bin_chunk_offset + CHUNK_HEADER_SIZE + bin_len]
    } else {
        &[] as &[u8]
    };

    let mut root: Value = serde_json::from_slice(json_bytes).expect("invalid GLB JSON");

    // Strip animations not in clip_names
    if let Some(animations) = root.get_mut("animations").and_then(|a| a.as_array_mut()) {
        animations.retain(|anim| {
            anim.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|name| clip_names.contains(&name.to_string()))
        });
    }

    // Rebuild binary at accessor granularity, then strip orphaned JSON entries
    let new_bin = rebuild_binary(&mut root, bin_data);
    strip_unreferenced_accessors(&mut root);

    let new_json_bytes = serde_json::to_vec(&root).unwrap();
    let json_padding = (4 - (new_json_bytes.len() % 4)) % 4;
    let padded_json_len = new_json_bytes.len() + json_padding;
    let bin_padding = (4 - (new_bin.len() % 4)) % 4;
    let padded_bin_len = new_bin.len() + bin_padding;
    let total_len =
        GLB_HEADER_SIZE + CHUNK_HEADER_SIZE + padded_json_len + CHUNK_HEADER_SIZE + padded_bin_len;

    // Reassemble GLB
    let mut out = Vec::with_capacity(total_len);
    out.write_all(&GLB_MAGIC.to_le_bytes()).unwrap();
    out.write_all(&2u32.to_le_bytes()).unwrap();
    out.write_all(&(total_len as u32).to_le_bytes()).unwrap();

    out.write_all(&(padded_json_len as u32).to_le_bytes())
        .unwrap();
    out.write_all(&JSON_CHUNK_TYPE.to_le_bytes()).unwrap();
    out.write_all(&new_json_bytes).unwrap();
    out.extend(std::iter::repeat_n(0x20u8, json_padding));

    out.write_all(&(padded_bin_len as u32).to_le_bytes())
        .unwrap();
    out.write_all(&BIN_CHUNK_TYPE.to_le_bytes()).unwrap();
    out.write_all(&new_bin).unwrap();
    out.extend(std::iter::repeat_n(0u8, bin_padding));

    fs::write(OUTPUT_GLB, &out).expect("failed to write player.glb");
}

/// Rebuild the binary buffer, keeping only accessor data that is still referenced.
///
/// The key insight: multiple accessors can share a single bufferView (common for
/// animation data). BufferView-level pruning won't help if even one accessor in
/// the view is still referenced. Instead, we copy data at accessor granularity
/// into fresh bufferViews, one per accessor.
fn rebuild_binary(root: &mut Value, bin_data: &[u8]) -> Vec<u8> {
    // Collect all referenced accessor indices
    let ref_accessors = collect_referenced_accessors(root);

    let accessors = match root.get("accessors").and_then(|a| a.as_array()) {
        Some(a) => a.clone(),
        None => return bin_data.to_vec(),
    };
    let buffer_views = match root.get("bufferViews").and_then(|b| b.as_array()) {
        Some(b) => b.clone(),
        None => return bin_data.to_vec(),
    };

    // Build new buffer: one new bufferView per referenced accessor
    let mut new_bin: Vec<u8> = Vec::new();

    // Track new bufferView entries and accessor updates
    let mut new_buffer_views: Vec<Value> = Vec::new();
    // Map: old accessor index -> new bufferView index
    let mut acc_to_new_bv: HashMap<usize, usize> = HashMap::new();

    // First pass: copy referenced accessor data into new buffer
    for (acc_idx, acc) in accessors.iter().enumerate() {
        if !ref_accessors.contains(&acc_idx) {
            continue;
        }

        let Some(bv_idx) = acc.get("bufferView").and_then(|v| v.as_u64()) else {
            continue;
        };
        let bv = &buffer_views[bv_idx as usize];
        let bv_offset = bv.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let bv_stride = bv.get("byteStride").and_then(|v| v.as_u64());

        let acc_offset = acc
            .get("byteOffset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let count = acc.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let comp_type = acc
            .get("componentType")
            .and_then(|v| v.as_u64())
            .unwrap_or(5126);
        let acc_type = acc
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("SCALAR");

        let element_size = component_byte_size(comp_type) * type_element_count(acc_type);
        let stride = bv_stride.map(|s| s as usize).unwrap_or(element_size);

        // Align new bufferView start to 4 bytes
        while new_bin.len() % 4 != 0 {
            new_bin.push(0);
        }
        let new_bv_offset = new_bin.len();

        if stride == element_size {
            // Tightly packed: single memcpy
            let src_start = bv_offset + acc_offset;
            let src_end = src_start + count * element_size;
            new_bin.extend_from_slice(&bin_data[src_start..src_end]);
        } else {
            // Strided: copy element by element, output tightly packed
            for i in 0..count {
                let src = bv_offset + acc_offset + i * stride;
                new_bin.extend_from_slice(&bin_data[src..src + element_size]);
            }
        }

        let new_bv_len = count * element_size;
        let new_bv_idx = new_buffer_views.len();
        acc_to_new_bv.insert(acc_idx, new_bv_idx);

        // Create new bufferView entry (no stride since we pack tightly)
        let mut bv_obj = Map::new();
        bv_obj.insert("buffer".to_string(), Value::from(0));
        bv_obj.insert("byteOffset".to_string(), Value::from(new_bv_offset));
        bv_obj.insert("byteLength".to_string(), Value::from(new_bv_len));
        // Preserve target if present (ARRAY_BUFFER / ELEMENT_ARRAY_BUFFER)
        if let Some(target) = bv.get("target") {
            bv_obj.insert("target".to_string(), target.clone());
        }
        new_buffer_views.push(Value::Object(bv_obj));
    }

    // Also preserve bufferViews referenced directly by images
    let mut image_bv_remap: HashMap<usize, usize> = HashMap::new();
    if let Some(images) = root.get("images").and_then(|i| i.as_array()) {
        for img in images {
            if let Some(old_bv) = img.get("bufferView").and_then(|v| v.as_u64()) {
                let old_bv = old_bv as usize;
                if image_bv_remap.contains_key(&old_bv) {
                    continue;
                }
                let bv = &buffer_views[old_bv];
                let offset = bv.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let length = bv.get("byteLength").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                while new_bin.len() % 4 != 0 {
                    new_bin.push(0);
                }
                let new_offset = new_bin.len();
                new_bin.extend_from_slice(&bin_data[offset..offset + length]);

                let new_idx = new_buffer_views.len();
                image_bv_remap.insert(old_bv, new_idx);

                let mut bv_obj = Map::new();
                bv_obj.insert("buffer".to_string(), Value::from(0));
                bv_obj.insert("byteOffset".to_string(), Value::from(new_offset));
                bv_obj.insert("byteLength".to_string(), Value::from(length));
                if let Some(mime) = bv.get("mimeType") {
                    bv_obj.insert("mimeType".to_string(), mime.clone());
                }
                new_buffer_views.push(Value::Object(bv_obj));
            }
        }
    }

    // Pad final buffer
    while new_bin.len() % 4 != 0 {
        new_bin.push(0);
    }

    // Update JSON: replace bufferViews array
    root.as_object_mut()
        .unwrap()
        .insert("bufferViews".to_string(), Value::Array(new_buffer_views));

    // Update accessors: point to new bufferViews, clear byteOffset (data starts at 0 in new bv)
    if let Some(accs) = root
        .get_mut("accessors")
        .and_then(|a| a.as_array_mut())
    {
        for (i, acc) in accs.iter_mut().enumerate() {
            if let Some(&new_bv) = acc_to_new_bv.get(&i) {
                let obj = acc.as_object_mut().unwrap();
                obj.insert("bufferView".to_string(), Value::from(new_bv));
                obj.remove("byteOffset"); // data starts at 0 in new bufferView
            }
        }
    }

    // Update image bufferView references
    if let Some(images) = root.get_mut("images").and_then(|i| i.as_array_mut()) {
        for img in images {
            if let Some(old_bv) = img.get("bufferView").and_then(|v| v.as_u64()) {
                if let Some(&new_bv) = image_bv_remap.get(&(old_bv as usize)) {
                    img.as_object_mut()
                        .unwrap()
                        .insert("bufferView".to_string(), Value::from(new_bv));
                }
            }
        }
    }

    // Update buffer length
    if let Some(buffers) = root.get_mut("buffers").and_then(|b| b.as_array_mut()) {
        if let Some(buf) = buffers.first_mut() {
            buf.as_object_mut()
                .unwrap()
                .insert("byteLength".to_string(), Value::from(new_bin.len()));
        }
    }

    new_bin
}

/// Walk the entire JSON tree and collect all accessor indices that are still referenced.
fn collect_referenced_accessors(root: &Value) -> HashSet<usize> {
    let mut refs: HashSet<usize> = HashSet::new();

    // Meshes
    if let Some(meshes) = root.get("meshes").and_then(|m| m.as_array()) {
        for mesh in meshes {
            if let Some(prims) = mesh.get("primitives").and_then(|p| p.as_array()) {
                for prim in prims {
                    if let Some(attrs) = prim.get("attributes").and_then(|a| a.as_object()) {
                        for (_, idx) in attrs {
                            if let Some(i) = idx.as_u64() {
                                refs.insert(i as usize);
                            }
                        }
                    }
                    if let Some(i) = prim.get("indices").and_then(|v| v.as_u64()) {
                        refs.insert(i as usize);
                    }
                    if let Some(targets) = prim.get("targets").and_then(|t| t.as_array()) {
                        for target in targets {
                            if let Some(attrs) = target.as_object() {
                                for (_, idx) in attrs {
                                    if let Some(i) = idx.as_u64() {
                                        refs.insert(i as usize);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Animations (only the kept ones)
    if let Some(animations) = root.get("animations").and_then(|a| a.as_array()) {
        for anim in animations {
            if let Some(samplers) = anim.get("samplers").and_then(|s| s.as_array()) {
                for sampler in samplers {
                    if let Some(i) = sampler.get("input").and_then(|v| v.as_u64()) {
                        refs.insert(i as usize);
                    }
                    if let Some(i) = sampler.get("output").and_then(|v| v.as_u64()) {
                        refs.insert(i as usize);
                    }
                }
            }
        }
    }

    // Skins
    if let Some(skins) = root.get("skins").and_then(|s| s.as_array()) {
        for skin in skins {
            if let Some(i) = skin.get("inverseBindMatrices").and_then(|v| v.as_u64()) {
                refs.insert(i as usize);
            }
        }
    }

    refs
}

/// Remove unreferenced accessors from the JSON and remap all accessor indices.
fn strip_unreferenced_accessors(root: &mut Value) {
    let ref_accessors = collect_referenced_accessors(root);

    let old_accessors = match root.get("accessors").and_then(|a| a.as_array()) {
        Some(a) => a.clone(),
        None => return,
    };

    // Build old→new index mapping, keeping only referenced accessors
    let mut remap: HashMap<usize, usize> = HashMap::new();
    let mut new_accessors: Vec<Value> = Vec::new();
    for (old_idx, acc) in old_accessors.into_iter().enumerate() {
        if ref_accessors.contains(&old_idx) {
            remap.insert(old_idx, new_accessors.len());
            new_accessors.push(acc);
        }
    }

    root.as_object_mut()
        .unwrap()
        .insert("accessors".to_string(), Value::Array(new_accessors));

    // Remap accessor indices everywhere they appear
    remap_mesh_accessors(root, &remap);
    remap_animation_accessors(root, &remap);
    remap_skin_accessors(root, &remap);
}

fn remap_mesh_accessors(root: &mut Value, remap: &HashMap<usize, usize>) {
    let Some(meshes) = root.get_mut("meshes").and_then(|m| m.as_array_mut()) else {
        return;
    };
    for mesh in meshes {
        let Some(prims) = mesh.get_mut("primitives").and_then(|p| p.as_array_mut()) else {
            continue;
        };
        for prim in prims {
            // attributes
            if let Some(attrs) = prim.get_mut("attributes").and_then(|a| a.as_object_mut()) {
                for (_, idx) in attrs.iter_mut() {
                    if let Some(old) = idx.as_u64() {
                        if let Some(&new) = remap.get(&(old as usize)) {
                            *idx = Value::from(new);
                        }
                    }
                }
            }
            // indices
            if let Some(old) = prim.get("indices").and_then(|v| v.as_u64()) {
                if let Some(&new) = remap.get(&(old as usize)) {
                    prim.as_object_mut()
                        .unwrap()
                        .insert("indices".to_string(), Value::from(new));
                }
            }
            // morph targets
            if let Some(targets) = prim.get_mut("targets").and_then(|t| t.as_array_mut()) {
                for target in targets {
                    if let Some(attrs) = target.as_object_mut() {
                        for (_, idx) in attrs.iter_mut() {
                            if let Some(old) = idx.as_u64() {
                                if let Some(&new) = remap.get(&(old as usize)) {
                                    *idx = Value::from(new);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn remap_animation_accessors(root: &mut Value, remap: &HashMap<usize, usize>) {
    let Some(animations) = root.get_mut("animations").and_then(|a| a.as_array_mut()) else {
        return;
    };
    for anim in animations {
        let Some(samplers) = anim.get_mut("samplers").and_then(|s| s.as_array_mut()) else {
            continue;
        };
        for sampler in samplers {
            let obj = sampler.as_object_mut().unwrap();
            for key in ["input", "output"] {
                if let Some(old) = obj.get(key).and_then(|v| v.as_u64()) {
                    if let Some(&new) = remap.get(&(old as usize)) {
                        obj.insert(key.to_string(), Value::from(new));
                    }
                }
            }
        }
    }
}

fn remap_skin_accessors(root: &mut Value, remap: &HashMap<usize, usize>) {
    let Some(skins) = root.get_mut("skins").and_then(|s| s.as_array_mut()) else {
        return;
    };
    for skin in skins {
        if let Some(old) = skin.get("inverseBindMatrices").and_then(|v| v.as_u64()) {
            if let Some(&new) = remap.get(&(old as usize)) {
                skin.as_object_mut()
                    .unwrap()
                    .insert("inverseBindMatrices".to_string(), Value::from(new));
            }
        }
    }
}

/// Extract clip names from `Animation::clip_name()` match arms.
fn parse_clip_names() -> Vec<String> {
    let source = fs::read_to_string(ANIMATION_RS).expect("failed to read animation.rs");

    let fn_start = source
        .find("fn clip_name(self)")
        .expect("clip_name() not found in animation.rs");
    let fn_body = &source[fn_start..];

    let mut depth = 0u32;
    let mut started = false;
    let mut end = fn_body.len();
    for (i, c) in fn_body.char_indices() {
        match c {
            '{' => {
                depth += 1;
                started = true;
            }
            '}' => {
                depth -= 1;
                if started && depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let block = &fn_body[..end];
    let mut names = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Self::") {
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest[start + 1..].find('"') {
                    names.push(rest[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }

    assert!(!names.is_empty(), "parsed zero clip names from animation.rs");
    names
}

/// Edge-collapse mesh simplification. Collapses shortest edges first using a
/// union-find remap, then rebuilds the index buffer discarding degenerate
/// triangles. Produces a new index buffer over the original vertex buffer.
pub(crate) fn simplify_indices(
    positions: &[[f32; 3]],
    indices: &[u32],
    target_count: usize,
) -> Vec<u32> {
    // Union-find: remap[v] points toward v's representative vertex.
    let mut remap: Vec<u32> = (0..positions.len() as u32).collect();

    fn find_root(remap: &mut [u32], mut v: u32) -> u32 {
        while remap[v as usize] != v {
            // Path compression
            remap[v as usize] = remap[remap[v as usize] as usize];
            v = remap[v as usize];
        }
        v
    }

    // Collect unique edges sorted by length (shortest first).
    let mut edges: Vec<(f32, u32, u32)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tri in indices.chunks(3) {
        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = (a.min(b), a.max(b));
            if seen.insert(key) {
                let pa = positions[a as usize];
                let pb = positions[b as usize];
                let dist_sq = (pa[0] - pb[0]).powi(2)
                    + (pa[1] - pb[1]).powi(2)
                    + (pa[2] - pb[2]).powi(2);
                edges.push((dist_sq, a, b));
            }
        }
    }
    edges.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Iteratively collapse shortest edges until we hit the target index count.
    // Check surviving triangle count every 16 collapses to amortize the cost.
    let target_tris = target_count / 3;
    let mut collapses_since_check = 0u32;

    for &(_, a, b) in &edges {
        let ra = find_root(&mut remap, a);
        let rb = find_root(&mut remap, b);
        if ra == rb {
            continue; // Already merged
        }
        remap[rb as usize] = ra;
        collapses_since_check += 1;

        if collapses_since_check >= 16 {
            collapses_since_check = 0;
            if count_surviving_tris(&remap, indices) <= target_tris {
                break;
            }
        }
    }

    // Rebuild index buffer, dropping degenerate triangles.
    let mut result = Vec::with_capacity(target_count);
    for tri in indices.chunks(3) {
        let a = find_root(&mut remap, tri[0]);
        let b = find_root(&mut remap, tri[1]);
        let c = find_root(&mut remap, tri[2]);
        if a != b && b != c && a != c {
            result.push(a);
            result.push(b);
            result.push(c);
        }
    }
    result
}

/// Count non-degenerate triangles after applying the union-find remap.
fn count_surviving_tris(remap: &[u32], indices: &[u32]) -> usize {
    fn find(remap: &[u32], mut v: u32) -> u32 {
        while remap[v as usize] != v {
            v = remap[v as usize];
        }
        v
    }

    let mut count = 0;
    for tri in indices.chunks(3) {
        let a = find(remap, tri[0]);
        let b = find(remap, tri[1]);
        let c = find(remap, tri[2]);
        if a != b && b != c && a != c {
            count += 1;
        }
    }
    count
}

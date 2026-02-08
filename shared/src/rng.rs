//! Deterministic RNG for combat resolution.
//!
//! Both client and server provide the same inputs → same output.
//! No clock sync needed.

/// Deterministic roll from attacker seed, target seed, and attack sequence number.
/// Returns a value in [0.0, 1.0).
pub fn deterministic_roll(attacker_seed: u64, target_seed: u64, attack_seq: u32) -> f32 {
    let mut hash: u64 = attacker_seed;
    hash ^= target_seed;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= attack_seq as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    (hash & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

/// Seed from identity bytes (first 8 bytes of SpacetimeDB Identity).
pub fn seed_from_identity(identity_bytes: &[u8]) -> u64 {
    let mut seed: u64 = 0;
    for (i, &b) in identity_bytes.iter().take(8).enumerate() {
        seed |= (b as u64) << (i * 8);
    }
    seed
}

/// Seed from a u64 ID (NPCs, entities without Identity).
pub fn seed_from_id(id: u64) -> u64 {
    id
}

/// Legacy deterministic random from timestamp and identity bytes.
/// Kept for server backward compatibility during transition.
pub fn deterministic_random_identity(timestamp_micros: i64, identity_bytes: &[u8]) -> f32 {
    let mut hash: u64 = timestamp_micros as u64;
    for &b in identity_bytes.iter().take(8) {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

/// Legacy deterministic random from timestamp and a u64 seed.
/// Kept for server backward compatibility during transition.
pub fn deterministic_random_u64(timestamp_micros: i64, seed: u64) -> f32 {
    let mut hash: u64 = timestamp_micros as u64;
    hash ^= seed;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= seed >> 32;
    hash = hash.wrapping_mul(0x100000001b3);
    (hash & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

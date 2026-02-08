//! Critical hit system preset

use crate::rules::OnPreHitRules;

pub fn crit() -> OnPreHitRules {
    OnPreHitRules(wasm_fantasia_shared::presets::crit::crit_rules())
}

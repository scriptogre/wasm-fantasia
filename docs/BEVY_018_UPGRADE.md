# Bevy 0.18 Upgrade Guide

## Why Upgrade?

Bevy 0.18 brings several features that would benefit this project:

### 1. Automatic Directional Navigation (Priority: High)
Built-in gamepad/keyboard UI navigation. Just add `AutoDirectionalNavigation` component to focusable elements - no custom implementation needed.

```rust
commands.spawn((
    Button,
    AutoDirectionalNavigation::default(),
));
```

### 2. Simplified Cargo Features (Priority: Medium)
High-level feature collections replace the verbose feature list:
- `3d` - Everything needed for 3D games
- `ui` - UI framework
- `scene` - Scene management
- `picking` - Input picking

Current 30+ features reduce to ~6.

### 3. First-Party Camera Controllers (Priority: Medium)
`FreeCamera` and `PanCamera` built into Bevy. Could replace `bevy_third_person_camera` dependency.

### 4. Other Improvements
- Font variations (weights, underlines, strikethroughs)
- Fullscreen materials for post-processing
- Better PBR shading
- Easy screenshot/video recording

---

## Dependency Updates Required

| Crate | Current | Target | Status |
|-------|---------|--------|--------|
| bevy | 0.17 | 0.18 | Available |
| bevy-inspector-egui | 0.35 | 0.36 | Available |
| bevy_hanabi | 0.17 | 0.18 | Available |
| bevy-tnua | 0.26 | 0.30 | Available (API changes) |
| bevy-tnua-avian3d | 0.8 | 0.10 | Available |
| bevy_enhanced_input | 0.20 | 0.23 | Available |
| bevy_seedling | 0.6 | 0.7 | Available |
| bevy_skein | 0.4 | 0.5 | Available |
| avian3d | 0.4 | 0.5 | Available |
| bevy_yarnspinner | 0.6-rc | 0.7 | Available |
| **bevy_third_person_camera** | 0.3 | ??? | **BLOCKER - No 0.18 version** |

---

## Blocking Issue: bevy_third_person_camera

Last updated 9 months ago. No Bevy 0.18 support.

### Options:

#### Option A: Replace with Custom Implementation
Write a simple third-person orbit camera. The current usage is straightforward:
- Orbit around player
- Mouse controls rotation
- Scroll controls zoom
- Elevated pitch for combat visibility

Estimated effort: 2-4 hours

#### Option B: Use Bevy's New Camera Controllers
Bevy 0.18 adds `FreeCamera` and `PanCamera`. These are designed for dev/editor use but could be adapted. However, they may not fit the "orbit around target" use case well.

#### Option C: Fork and Update
Fork `bevy_third_person_camera` and update for 0.18. The crate is simple (~500 lines), so updating shouldn't be too difficult.

**Recommendation**: Option A or C. The camera logic is simple enough that a custom implementation gives us more control and removes the dependency risk.

---

## Code Changes Required

### 1. bevy-tnua API Changes
`TnuaController` now requires generics:
```rust
// Before
TnuaController::default()

// After
TnuaController::<TnuaBuiltinWalk>::default()
```

`TnuaBuiltinDashState` moved - need to update imports.

### 2. Bevy Feature Flags
Replace verbose feature list:
```toml
# Before (30+ features)
features = ["std", "zstd_rust", "sysinfo_plugin", ...]

# After
features = ["default_app", "default_platform", "3d", "ui", "scene", "picking", "jpeg", "serialize", "hotpatching"]
```

### 3. Dev Features
```toml
# Before
dev = ["bevy/bevy_dev_tools", "bevy/bevy_ui_debug", "bevy/dynamic_linking", "bevy/track_location"]

# After
dev = ["bevy/dev", "bevy/dynamic_linking"]
```

### 4. Web Features
```toml
# Before
web = ["bevy/webgpu", ...]

# After
web = ["bevy/web", ...]
```

### 5. Audio Types
Some audio types may have moved or renamed. `SfxBus` references need checking.

### 6. UI Debug Options
`UiDebugOptions` may have been renamed or moved.

---

## Migration Steps

1. **Resolve camera dependency** (blocker)
   - Implement custom orbit camera OR fork and update bevy_third_person_camera

2. **Update Cargo.toml**
   - Update all dependency versions
   - Simplify Bevy feature flags
   - Remove git patches that are no longer needed

3. **Fix compilation errors**
   - Update bevy-tnua usage (generics)
   - Fix import paths
   - Update any renamed/moved types

4. **Add UI navigation**
   - Add `AutoDirectionalNavigation` to buttons
   - Configure `AutoNavigationConfig` if needed
   - Test gamepad navigation

5. **Test thoroughly**
   - Native build
   - Web build
   - Gamepad support
   - All game features

---

## Cargo.toml Target State

```toml
[dependencies.bevy]
version = "^0.18"
default-features = false
features = [
    "default_app",
    "default_platform",
    "3d",
    "ui",
    "scene",
    "picking",
    "jpeg",
    "experimental_bevy_ui_widgets",
    "bevy_gilrs",
    "serialize",
    "hotpatching",
]

[dependencies]
bevy-inspector-egui = { version = "^0.36", optional = true }
bevy_skein = "^0.5"
avian3d = { version = "^0.5", features = ["debug-plugin", "3d", "parallel", "collider-from-mesh"] }
bevy_seedling = { version = "^0.7", features = ["hrtf", "ogg", "web_audio"], optional = true }
bevy_hanabi = { version = "0.18", default-features = false, features = ["3d"], optional = true }
bevy-tnua = "^0.30"
bevy-tnua-avian3d = "^0.10"
bevy_enhanced_input = { version = "^0.23", features = ["serialize"] }
bevy_yarnspinner = "^0.7"
bevy_yarnspinner_example_dialogue_view = "^0.7"
# bevy_third_person_camera - REMOVED, replaced with custom implementation
```

---

## Resources

- [Bevy 0.18 Release Notes](https://bevyengine.org/news/bevy-0-18/)
- [Bevy 0.17 to 0.18 Migration Guide](https://bevyengine.org/learn/migration-guides/0-17-to-0-18/)
- [AutoDirectionalNavigation docs](https://docs.rs/bevy/0.18.0/bevy/ui/widget/struct.AutoDirectionalNavigation.html)

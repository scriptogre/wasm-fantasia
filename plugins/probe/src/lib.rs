use game_core::{
    fury,
    runtime::{Combatant, Hit, Intent, ScriptEngine},
};
use std::{cell::RefCell, hint::black_box};
use wasm_component_layer::*;

pub struct ComponentHost {
    store: Store<(), wasmi_runtime_layer::Engine>,
    hit: Func,
    hits: Func,
    bonus: TypedFunc<i64, u16>,
}

impl ComponentHost {
    pub fn load(bytes: &[u8]) -> Self {
        let engine = Engine::new(wasmi_runtime_layer::Engine::default());
        let mut store = Store::new(&engine, ());
        let component = Component::new(&engine, bytes).unwrap();
        let instance = Linker::default()
            .instantiate(&mut store, &component)
            .unwrap();
        let interface = instance
            .exports()
            .instance(&"fantasia:mechanics/fury@0.1.0".try_into().unwrap())
            .unwrap();
        Self {
            hit: interface.func("on-hit").unwrap().clone(),
            hits: interface.func("on-hits").unwrap().clone(),
            bonus: interface.func("bonus-percent").unwrap().typed().unwrap(),
            store,
        }
    }

    pub fn hit(&mut self, stacks: i64, is_crit: bool) -> [u64; 3] {
        self.invoke(
            self.hit.clone(),
            &[Value::S64(stacks), Value::Bool(is_crit)],
        )
    }

    pub fn hits(&mut self, stacks: i64, hits: &[bool]) -> [u64; 3] {
        let list = List::new(
            ListType::new(ValueType::Bool),
            hits.iter().copied().map(Value::Bool),
        )
        .unwrap();
        self.invoke(
            self.hits.clone(),
            &[Value::S64(stacks), Value::U64(1_000_000), Value::List(list)],
        )
    }

    fn invoke(&mut self, function: Func, args: &[Value]) -> [u64; 3] {
        let mut results = [Value::Bool(false)];
        function.call(&mut self.store, args, &mut results).unwrap();
        let Value::Record(state) = &results[0] else {
            panic!("Expected state record")
        };
        let Some(Value::S64(stacks)) = state.field("stacks") else {
            panic!("Expected stacks")
        };
        let Some(Value::U64(micros)) = state.field("remaining-micros") else {
            panic!("Expected duration")
        };
        let bonus = self.bonus.call(&mut self.store, stacks).unwrap();
        [stacks as u64, micros, bonus as u64]
    }
}

thread_local! {
    static COMPONENT_BYTES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static COMPONENT: RefCell<Option<ComponentHost>> = const { RefCell::new(None) };
    static RUNE: RefCell<Option<ScriptEngine>> = const { RefCell::new(None) };
    static LAST: RefCell<[u64; 3]> = const { RefCell::new([0; 3]) };
}

#[cfg(feature = "browser")]
#[link(wasm_import_module = "plugin")]
unsafe extern "C" {
    fn hit(stacks: i64, is_crit: u32, output: *mut u64);
    fn hits(stacks: i64, hits: *const bool, len: usize, output: *mut u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn reserve_component(len: usize) -> *mut u8 {
    COMPONENT_BYTES.with(|bytes| {
        let mut bytes = bytes.borrow_mut();
        bytes.resize(len, 0);
        bytes.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn load_component() {
    COMPONENT_BYTES.with(|bytes| {
        COMPONENT.with(|host| *host.borrow_mut() = Some(ComponentHost::load(&bytes.borrow())))
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn setup_rune() {
    RUNE.with(|engine| {
        *engine.borrow_mut() =
            Some(ScriptEngine::new(include_str!("../fixtures/stacking.rune")).unwrap())
    });
}

fn combatant(stacks: i64) -> Combatant {
    Combatant {
        id: 1,
        pos_x: 0.0,
        pos_y: 0.0,
        pos_z: 0.0,
        dir_x: 0.0,
        dir_z: 1.0,
        health: 100.0,
        max_health: 100.0,
        attack_damage: 25.0,
        crit_chance: 0.2,
        crit_multiplier: 2.5,
        knockback_force: 6.0,
        attack_range: 3.6,
        attack_arc: 150.0,
        attack_speed: 1.0,
        fury_stacks: stacks,
        attack_speed_bonus: 0.0,
        cooldown_ready: true,
        speed: 1.0,
    }
}

pub fn evaluate(backend: u32, stacks: i64, is_crit: bool) -> [u64; 3] {
    match backend {
        0 => RUNE.with(|engine| {
            let (_, intents, effects) = engine
                .borrow()
                .as_ref()
                .unwrap()
                .call_hit_hook(
                    "on_hit",
                    combatant(stacks),
                    combatant(0),
                    Hit {
                        damage: 25.0,
                        knockback: 6.0,
                        is_crit,
                    },
                    0.5,
                )
                .unwrap();
            assert!(effects.is_empty());
            let [
                Intent::StatSet { value: stacks, .. },
                Intent::StatSet { value: bonus, .. },
                Intent::BuffAdded { duration, .. },
            ] = intents.as_slice()
            else {
                panic!("Wrong Rune outputs")
            };
            [
                *stacks as u64,
                (*duration * 1_000_000.0) as u64,
                (*bonus * 100.0).round() as u64,
            ]
        }),
        1 => COMPONENT.with(|host| host.borrow_mut().as_mut().unwrap().hit(stacks, is_crit)),
        #[cfg(feature = "browser")]
        2 => {
            let mut output = [0u64; 3];
            unsafe {
                hit(stacks, u32::from(is_crit), output.as_mut_ptr());
            }
            output
        }
        3 => {
            let state = fury::on_hit(stacks, is_crit);
            [
                state.stacks as u64,
                state.remaining_micros,
                fury::bonus_percent(state.stacks) as u64,
            ]
        }
        _ => panic!("Unknown backend"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn check(backend: u32, stacks: i64, is_crit: u32) -> *const u64 {
    let result = evaluate(backend, stacks, is_crit != 0);
    LAST.with(|last| {
        *last.borrow_mut() = result;
        last.borrow().as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn benchmark(backend: u32, repetitions: u32, seed: u32) -> u64 {
    let mut checksum = 0;
    for i in 0..repetitions {
        let input = seed.wrapping_add(i);
        let result = black_box(evaluate(backend, (input % 13) as i64, input % 5 == 0));
        checksum += result[0] + result[1] + result[2];
    }
    checksum
}

pub fn evaluate_batch(backend: u32, stacks: i64, len: u32, seed: u32) -> [u64; 3] {
    let flags: Vec<bool> = (0..len).map(|i| seed.wrapping_add(i) % 5 == 0).collect();
    match backend {
        0 => {
            let state = fury::elapse(
                fury::Fury {
                    stacks,
                    remaining_micros: 1_000_000,
                },
                0,
            );
            let mut result = [
                state.stacks as u64,
                state.remaining_micros,
                fury::bonus_percent(state.stacks) as u64,
            ];
            for critical in flags {
                result = evaluate(0, result[0] as i64, critical);
            }
            result
        }
        1 => COMPONENT.with(|host| host.borrow_mut().as_mut().unwrap().hits(stacks, &flags)),
        #[cfg(feature = "browser")]
        2 => {
            let mut output = [0u64; 3];
            unsafe {
                hits(stacks, flags.as_ptr(), flags.len(), output.as_mut_ptr());
            }
            output
        }
        3 => {
            let mut state = fury::elapse(
                fury::Fury {
                    stacks,
                    remaining_micros: 1_000_000,
                },
                0,
            );
            for critical in flags {
                state = fury::on_hit(state.stacks, critical);
            }
            [
                state.stacks as u64,
                state.remaining_micros,
                fury::bonus_percent(state.stacks) as u64,
            ]
        }
        _ => panic!("Unknown backend"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn check_batch(backend: u32, stacks: i64, len: u32, seed: u32) -> *const u64 {
    let result = evaluate_batch(backend, stacks, len, seed);
    LAST.with(|last| {
        *last.borrow_mut() = result;
        last.borrow().as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn benchmark_batch(backend: u32, repetitions: u32, len: u32, seed: u32) -> u64 {
    let mut checksum = 0;
    for i in 0..repetitions {
        let result = black_box(evaluate_batch(
            backend,
            (seed.wrapping_add(i) % 13) as i64,
            len,
            seed.wrapping_add(i),
        ));
        checksum += result[0] + result[1] + result[2];
    }
    checksum
}

#[cfg(feature = "server")]
#[spacetimedb::reducer]
pub fn verify_component(_ctx: &spacetimedb::ReducerContext, bytes: Vec<u8>) {
    let mut plugin = ComponentHost::load(&bytes);
    for (stacks, is_crit) in [
        (0, false),
        (0, true),
        (11, true),
        (i64::MIN, false),
        (i64::MAX, true),
    ] {
        assert_eq!(plugin.hit(stacks, is_crit), evaluate(3, stacks, is_crit));
    }
    for length in [0, 1, 8, 64] {
        let flags: Vec<bool> = (0..length).map(|i| (17 + i) % 5 == 0).collect();
        for stacks in [0, 5, 12] {
            assert_eq!(
                plugin.hits(stacks, &flags),
                evaluate_batch(3, stacks, length, 17)
            );
        }
    }
    spacetimedb::log::info!("Fury component: 17 checks passed inside SpacetimeDB");
}

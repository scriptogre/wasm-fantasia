#![no_std]

extern crate alloc;
use alloc::vec::Vec;

#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

// The wasip2 standard library normally exports this allocation entry point.
#[unsafe(no_mangle)]
unsafe extern "C" fn cabi_realloc(
    ptr: *mut u8,
    old_len: usize,
    align: usize,
    new_len: usize,
) -> *mut u8 {
    use alloc::alloc::{Layout, alloc, realloc};
    if old_len == 0 && new_len == 0 {
        return align as *mut u8;
    }
    let result = unsafe {
        if old_len == 0 {
            alloc(Layout::from_size_align_unchecked(new_len, align))
        } else {
            realloc(
                ptr,
                Layout::from_size_align_unchecked(old_len, align),
                new_len,
            )
        }
    };
    if result.is_null() {
        core::arch::wasm32::unreachable();
    }
    result
}

#[path = "../../../core/src/fury.rs"]
mod fury;

wit_bindgen::generate!({ path: "../wit", world: "mechanic" });

use exports::fantasia::mechanics::fury::{Guest, State};

struct Plugin;

impl Guest for Plugin {
    fn on_hits(stacks: i64, remaining_micros: u64, hits: Vec<bool>) -> State {
        let mut state = fury::elapse(
            fury::Fury {
                stacks,
                remaining_micros,
            },
            0,
        );
        for is_crit in hits {
            state = fury::on_hit(state.stacks, is_crit);
        }
        State {
            stacks: state.stacks,
            remaining_micros: state.remaining_micros,
        }
    }

    fn on_hit(stacks: i64, is_crit: bool) -> State {
        let result = fury::on_hit(stacks, is_crit);
        State {
            stacks: result.stacks,
            remaining_micros: result.remaining_micros,
        }
    }

    fn elapse(current: State, micros: u64) -> State {
        let result = fury::elapse(
            fury::Fury {
                stacks: current.stacks,
                remaining_micros: current.remaining_micros,
            },
            micros,
        );
        State {
            stacks: result.stacks,
            remaining_micros: result.remaining_micros,
        }
    }

    fn bonus_percent(stacks: i64) -> u16 {
        fury::bonus_percent(stacks)
    }
}

export!(Plugin);

#![no_std]
extern crate alloc;

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

pub mod errors;
pub mod graceful_degradation;
pub mod namespace;
pub mod scaling;
pub mod weighted_rate;

#[cfg(test)]
mod namespace_test;

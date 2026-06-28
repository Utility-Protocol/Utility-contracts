#![no_std]
extern crate alloc;

pub mod errors;
pub mod namespace;
pub mod scaling;
pub mod weighted_rate;

#[cfg(test)]
mod namespace_test;

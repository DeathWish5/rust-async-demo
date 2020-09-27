#![no_std]
#![feature(wake_trait)]

extern crate alloc;

pub mod task;
pub mod executor;

pub use executor::*;
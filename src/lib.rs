#![feature(return_position_impl_trait_in_trait)]
#![cfg_attr(not(test), no_std)]
mod compiler;
mod dwarf;
mod fp;
mod arch;

extern crate alloc;

pub use compiler::*;
pub use dwarf::*;
use core::iter::Iterator;
pub use fp::FramePointTracer;
pub trait Symbol {
    fn addr(&self) -> usize;
    fn name(&self) -> &str;
}

pub struct TraceInfo{
    pub func_name:&'static str,
    pub func_addr:usize,
    pub bias:usize,
}

pub trait Tracer{
    fn trace(&self)->impl Iterator<Item=TraceInfo> +'_;
}



pub trait TracerProvider{
    fn address2symbol(&self, addr: usize) -> Option<(usize, &'static str)>;
}
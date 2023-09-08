use crate::utils::read_value;
use crate::{TraceInfo, Tracer, TracerProvider};
use core::arch::asm;

pub struct FramePointTracer<T> {
    provider: T,
}

impl<T: TracerProvider> FramePointTracer<T> {
    pub fn new(provider: T) -> Self {
        Self { provider }
    }
}

impl<T: TracerProvider> Tracer for FramePointTracer<T> {
    fn trace(&self) -> impl Iterator<Item = TraceInfo> + '_ {
        FramePointTracerIterator {
            fp: 0,
            provider: &self.provider,
        }
    }
}

struct FramePointTracerIterator<'a, T> {
    fp: usize,
    provider: &'a T,
}

impl<T: TracerProvider> Iterator for FramePointTracerIterator<'_, T> {
    type Item = TraceInfo;
    fn next(&mut self) -> Option<Self::Item> {
        if self.fp == 0 {
            let fp: usize;
            unsafe {
                asm!("mv {},s0",out(reg)fp);
            }
            self.fp = fp;
        }
        let ra = read_value(self.fp - 8);
        let func_info = self.provider.address2symbol(ra)?;
        let new_fp = read_value(self.fp - 16);
        self.fp = new_fp;
        Some(TraceInfo {
            func_name: func_info.1,
            func_addr: func_info.0,
            bias: ra - func_info.0,
        })
    }
}

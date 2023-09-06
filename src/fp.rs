use crate::{TraceInfo, Tracer, TracerProvider};
use crate::arch::fp;

pub struct FramePointTracer<T>{
    provider:T
}

impl<T: TracerProvider> FramePointTracer<T> {
    pub fn new(provider:T) -> Self {
        Self{
            provider,
        }
    }
}

impl<T: TracerProvider> Tracer for FramePointTracer<T>{
    fn trace(&self) -> impl Iterator<Item=TraceInfo> + '_ {
        let fp = fp();
        FramePointTracerIterator{
            fp,
            provider:&self.provider,
        }
    }
}

struct FramePointTracerIterator<'a,T>{
    fp:usize,
    provider:&'a T,
}

impl<T: TracerProvider> Iterator for FramePointTracerIterator<'_,T>{
    type Item = TraceInfo;
    fn next(&mut self) -> Option<Self::Item> {
        let ra = unsafe {((self.fp -8) as *const usize).read_volatile()};
        let func_info = self.provider.address2symbol(ra);
        let new_fp = unsafe {((self.fp-16) as *const usize).read_volatile()};
        self.fp = new_fp;
        func_info.map(|(addr,name)|{
            TraceInfo{
                func_name:name,
                func_addr:addr,
                bias:ra-addr,
            }
        })
    }
}
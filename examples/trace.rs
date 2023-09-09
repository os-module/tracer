use tracer::{FramePointTracer, Tracer, TracerProvider};

fn main() {
    let tracer = FramePointTracer::new(Provider);
    tracer.trace().for_each(|x| {
        println!(
            "func_name: {}, func_addr: {:#x}, bias: {:#x}",
            x.func_name, x.func_addr, x.bias
        );
    });
}
struct Provider;
impl TracerProvider for Provider {
    fn address2symbol(&self, addr: usize) -> Option<(usize, &'static str)> {
        println!("addr: {}", addr);
        None
    }
}

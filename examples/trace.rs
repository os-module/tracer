use tracer::{CompilerTracer, FramePointTracer, Tracer, TracerProvider};

fn main() {
    s1(1);
}
struct Provider;
impl TracerProvider for Provider {
    fn address2symbol(&self, addr: usize) -> Option<(usize, &'static str)> {
        println!("addr: {}", addr);
        None
    }
}

fn _add(a: usize, b: usize) -> usize {
    a + b
}
fn s1(a1: usize) -> usize {
    println!("s1: {a1}");
    let x = _add(a1, 1);
    println!("{}", s2(x));
    0
}

fn s2(a1: usize) -> usize {
    println!("s2: {a1}");
    let _x = _add(a1, 1);
    s3();
    0
}

fn s3() {
    let tracer = FramePointTracer::new(Provider);
    tracer.trace().for_each(|x| {
        println!(
            "func_name: {}, func_addr: {:#x}, bias: {:#x}",
            x.func_name, x.func_addr, x.bias
        );
    });
    let tracer = CompilerTracer::new(Provider);
    tracer.trace().for_each(|x| {
        println!(
            "func_name: {}, func_addr: {:#x}, bias: {:#x}",
            x.func_name, x.func_addr, x.bias
        );
    });
}

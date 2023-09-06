use core::arch::asm;

#[inline]
pub fn fp() ->usize{
    let mut fp: usize;
    unsafe{
        asm!("mv {}, s0", out(reg) fp);
    }
    fp
}
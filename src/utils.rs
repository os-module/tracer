pub fn read_instruction(addr: usize) -> u32 {
    unsafe { ((addr) as *const u32).read_volatile() }
}
pub fn read_instruction_short(addr: usize) -> u16 {
    unsafe { ((addr) as *const u16).read_volatile() }
}

pub fn read_value(addr: usize) -> usize {
    unsafe { ((addr) as *const usize).read_volatile() }
}

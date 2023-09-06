use bit_field::BitField;
use core::arch::asm;
use log::{info, trace};
use crate::{TraceInfo, Tracer, TracerProvider};

// 在函数第一条指令，开辟栈空间
// 指令 addi sp,sp,imm 4字节
// imm[11:0] rs 000 rd 0010011
// 有三种二字节的压缩指令
// c.addi rd,imm
// 000 [imm5] rd [imm4-0] 01
// c.addi16sp imm
// 011 [imm9] 00010 [imm4|6|8|7|5] 01
#[derive(Debug,Copy, Clone)]
enum InstructionSp {
    Addi(u32),
    CAddi(u32),
    CAddi16Sp(u32),
}
impl InstructionSp {
    fn new(ins: u32) -> Self {
        Self::try_new(ins,|imm|{imm<0}).unwrap()
    }
    fn try_new(ins:u32,f:fn(i32)->bool)->Option<Self>{
        let opcode = ins.get_bits(0..7);
        match opcode {
            0b0010011 => {
                // 高12位符号扩展
                let mut imm = ins.get_bits(20..32);
                for i in 12..32 {
                    imm.set_bit(i, imm.get_bit(11));
                }
                let imm = imm as i32;
                if f(imm){
                   Some(InstructionSp::Addi((-imm) as u32))
                }else {
                    None
                }
            }
            _ => {
                let short_ins = ins.get_bits(0..16);
                let high = short_ins.get_bits(13..16);
                let low = short_ins.get_bits(0..2);
                match (high, low) {
                    (0b000, 0b01) => {
                        // 保证是sp
                        let rd = short_ins.get_bits(7..12);
                        if rd != 2 {
                            return None;
                        }
                        let mut imm = 0;
                        imm.set_bits(0..5, short_ins.get_bits(2..7));
                        imm.set_bit(5, short_ins.get_bit(12));
                        // 符号扩展
                        for i in 6..32 {
                            imm.set_bit(i, imm.get_bit(5));
                        }
                        let imm = imm as i32;
                        trace!("[CADDI] {:#b}", imm);
                        if f(imm) {
                            Some(InstructionSp::CAddi((-imm) as u32))
                        }else {
                            None
                        }
                    }
                    (0b011, 0b01) => {
                        let flag = short_ins.get_bits(7..=11);
                        if flag != 0b00010 {
                            return None;
                        }
                        let mut imm = 0u32;
                        imm.set_bit(9, short_ins.get_bit(12));
                        imm.set_bit(8, short_ins.get_bit(4));
                        imm.set_bit(7, short_ins.get_bit(3));
                        imm.set_bit(6, short_ins.get_bit(5));
                        imm.set_bit(5, short_ins.get_bit(2));
                        imm.set_bit(4, short_ins.get_bit(6));
                        for i in 10..32 {
                            imm.set_bit(i, imm.get_bit(9));
                        }
                        let imm = imm as i32;
                        trace!("sp_size: {}", -imm);
                        if f(imm){
                           Some(InstructionSp::CAddi16Sp((-imm) as u32))
                        }else {
                            None
                        }
                    }
                    _ => None,
                }
            }
        }
    }
}

fn check_sd_ra(ins: u32) -> Option<u32> {
    // 检查指令是否是存储ra
    let opcode = ins.get_bits(0..7);
    return match opcode {
        0b0100011 => {
            // 四字节的sd指令
            let func = ins.get_bits(12..=14);
            if func != 0b011 {
                return None;
            }
            let rd = ins.get_bits(15..=19); // sp
            let rt = ins.get_bits(20..=24); // ra
            if rd != 2 || rt != 1 {
                return None;
            }
            let mut imm = 0u32;
            imm.set_bits(0..=4, ins.get_bits(7..=11));
            imm.set_bits(5..=11, ins.get_bits(25..=31));
            for i in 12..32 {
                imm.set_bit(i, imm.get_bit(11));
            }
            let imm = imm as isize;
            assert!(imm > 0);
            Some(imm as u32)
        }
        _ => {
            // 2字节的sd指令
            // c.sdsp
            // 111 [uimm5:3 8:6] rt 10
            let short_ins = ins.get_bits(0..16);
            let high = short_ins.get_bits(13..16);
            let low = short_ins.get_bits(0..2);
            match (high, low) {
                (0b111, 0b10) => {
                    let mut imm = 0u32;
                    imm.set_bits(3..6, short_ins.get_bits(10..13));
                    imm.set_bits(6..9, short_ins.get_bits(7..10));
                    Some(imm)
                }
                (_, _) => None,
            }
        }
    };
}


pub struct CompilerTracer<T>{
    provider:T,
}

pub struct CompilerTracerIterator<'a,T>{
    /// The first instruction address of the function
    f_ins_addr:usize,
    /// The sp value
    sp:usize,
    /// The ra value
    ra:usize,
    provider:&'a T,
}
impl<T> CompilerTracer<T> {
    pub fn new(provider:T) -> Self {
        Self{
            provider,
        }
    }
}



impl <T: TracerProvider> Tracer for CompilerTracer<T>{
    fn trace(&self) -> impl Iterator<Item=TraceInfo> + '_ {
        CompilerTracerIterator{
            f_ins_addr:0,
            sp:0,
            ra: 0,
            provider:&self.provider,
        }
    }
}

impl<T:TracerProvider> Iterator for CompilerTracerIterator<'_,T>{
    type Item = TraceInfo;

    fn next(&mut self) -> Option<Self::Item> {
        if self.sp == 0{
            // 第一次调用
            let trace_addr = Self::next as usize;
            self.sp = {
                let t: usize;
                unsafe {
                    asm!("mv {},sp",out(reg)t);
                }
                t
            };
            self.f_ins_addr = trace_addr;
            self.ra = trace_addr;
        }
        let first_ins = read_instruction(self.f_ins_addr);
        info!("f_ins_addr: {:#x}, short_ins:{:#x}", self.f_ins_addr,read_instruction_short(self.f_ins_addr));
        trace!(
            "first_ins: {:#x} {:#b}",
            first_ins.get_bits(0..16),
            first_ins.get_bits(0..16)
        );
        let ans = InstructionSp::new(first_ins);
        let (next_ins_addr,next_ins, mut stack_size) = match ans {
            InstructionSp::Addi(size) => {
                // 四字节指令
                (self.f_ins_addr+4,read_instruction(self.f_ins_addr + 4), size)
            }
            InstructionSp::CAddi(size) | InstructionSp::CAddi16Sp(size) => {
                // 双字节指令
                (self.f_ins_addr+2,read_instruction(self.f_ins_addr+2), size)
            }
        };
        info!("next_ins:{:#x}, stack_size: {}, ra:{:#x}", next_ins, stack_size,self.ra);
        // 第二条指令就是记录有ra的值
        // 需要确保第二条指令是否是存储ra
        if check_sd_ra(next_ins).is_none() {
            return None;
        }

        // 在一些函数中，可能不止在第一条指令中调用了addi sp,sp,imm
        // 因此我们需要扫描函数开始到ra之间的指令，检查是否还出现了addi sp,sp,imm

        let mut start = next_ins_addr;
        let end = self.ra;
        while start < end{
            let short_ins = read_instruction_short(start);
            if is_caddi16sp(short_ins) || is_caddi(short_ins){
                let ins = InstructionSp::try_new(short_ins as u32,|imm|{imm < 0});
                if ins.is_none(){
                    start += 2;
                    continue
                }
                let ins = ins.unwrap();
                info!("addr: {:#x}, scan short_ins: {:?}", start,ins);
                match ins {
                    InstructionSp::Addi(size) => {
                        stack_size += size;
                    }
                    InstructionSp::CAddi(size) | InstructionSp::CAddi16Sp(size) => {
                        stack_size += size;
                    }
                }
                start += 2;
            } else if maybe_is_addi(short_ins){
                let ins = read_instruction(start);
                let ins = InstructionSp::try_new(ins,|imm|{imm < 0});
                if ins.is_none(){
                    start += 4;
                    continue
                }
                let ins = ins.unwrap();
                info!("addr: {:#x}, scan ins: {:?}",start, ins);
                match ins {
                    InstructionSp::Addi(x) => {
                        stack_size += x;
                    }
                    _ => {}
                }
                start += 4;
            }else {
                start += 2;
            }
        }
        let ra_addr = self.sp + stack_size as usize - 8;
        let ra = read_value(ra_addr); // 8字节存储
        info!("after scan, stack size :{} ra_addr:{:#x}, ra: {:#x}",stack_size ,ra_addr,ra);
        let father_func_info = self.provider.address2symbol(ra);
        if father_func_info.is_none() {
            return None;
        }
        let father_func_info = father_func_info.unwrap();
        self.f_ins_addr = father_func_info.0;
        self.sp += stack_size as usize;  // back to father stack
        self.ra = ra;
        Some(TraceInfo{
            func_name:father_func_info.1,
            func_addr:father_func_info.0,
            bias:ra-father_func_info.0,
        })
    }
}


fn is_caddi(ins:u16)->bool{
    let high = ins.get_bits(13..16);
    let low = ins.get_bits(0..2);
    match (high, low) {
        (0b000, 0b01) => {
            // 保证是sp
            let rd = ins.get_bits(7..12);
            if rd != 2 {
                return false;
            }
            true
        }
        _ => false,
    }
}

fn is_caddi16sp(ins:u16)->bool{
    let high = ins.get_bits(13..16);
    let low = ins.get_bits(0..2);
    match (high, low) {
        (0b011, 0b01) => {
            let flag = ins.get_bits(7..=11);
            if flag != 0b00010 {
                return false;
            }
            true
        }
        _ => false,
    }
}

/// imm\[11:0] rs 000 rd 0010011
///
/// 0001\[0 000 00010 0010011]
/// 001--0x0113
fn maybe_is_addi(ins:u16)->bool{
    ins == 0x113
}

fn read_instruction(addr:usize)->u32{
    unsafe {((addr) as *const u32).read_volatile()}
}
fn read_instruction_short(addr:usize)->u16{
    unsafe {((addr) as *const u16).read_volatile()}
}

fn read_value(addr:usize)->usize{
    unsafe {((addr) as *const usize).read_volatile()}
}
#![no_std]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use bit_field::BitField;
use core::arch::asm;
use log::{trace};


pub trait Symbol{
    fn addr(&self)->usize;
    fn name(&self)->&str;
}

// 在函数第一条指令，开辟栈空间
// 指令 addi sp,sp,imm 4字节
// imm[11:0] rs 000 rd 0010011
// 有三种二字节的压缩指令
// c.addi rd,imm
// 000 [imm5] rd [imm4-0] 01
// c.addi16sp imm
// 011 [imm9] 00010 [imm4|6|8|7|5] 01

enum InstructionSp {
    Addi(u32),
    CAddi(u32),
    CAddi16Sp(u32),
    Unknown,
}

impl InstructionSp {
    fn new(ins: u32) -> Self {
        let opcode = ins.get_bits(0..7);
        match opcode {
            0b0010011 => {
                //高12位符号扩展
                let mut imm = ins.get_bits(20..32);
                for i in 12..32 {
                    imm.set_bit(i, imm.get_bit(11));
                }
                let imm = imm as i32;
                assert!(imm < 0);
                InstructionSp::Addi((-imm) as u32)
            }
            _ => {
                let short_ins = ins.get_bits(0..16);
                let high = short_ins.get_bits(13..16);
                let low = short_ins.get_bits(0..2);
                match (high, low) {
                    (0b000, 0b01) => {
                        //保证是sp
                        let rd = short_ins.get_bits(7..12);
                        if rd != 2 {
                            return InstructionSp::Unknown;
                        }
                        let mut imm = 0;
                        imm.set_bits(0..5, short_ins.get_bits(2..7));
                        imm.set_bit(5, short_ins.get_bit(12));
                        //符号扩展
                        for i in 6..32 {
                            imm.set_bit(i, imm.get_bit(5));
                        }
                        let imm = imm as i32;
                        trace!("[CADDI] {:#b}", imm);
                        assert!(imm < 0);
                        InstructionSp::CAddi((-imm) as u32)
                    }
                    (0b011, 0b01) => {
                        let flag = short_ins.get_bits(7..=11);
                        if flag != 0b00010 {
                            return InstructionSp::Unknown;
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
                        assert!(imm < 0);
                        InstructionSp::CAddi16Sp((-imm) as u32)
                    }
                    _ => InstructionSp::Unknown,
                }
            }
        }
    }
}

fn sd_ra(ins: u32) -> Option<u32> {
    //检查指令是否是存储ra
    let opcode = ins.get_bits(0..7);
    return match opcode {
        0b0100011 => {
            // 四字节的sd指令
            let func = ins.get_bits(12..=14);
            if func != 0b011 {
                return None;
            }
            let rd = ins.get_bits(15..=19); //sp
            let rt = ins.get_bits(20..=24); //ra
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


pub unsafe fn my_trace<T:Symbol>(symbol: Vec<T>) -> Vec<String> {
    let s = my_trace::<T> as usize;
    // 函数的第一条指令
    let mut ins = s as *const u32;
    let mut sp = {
        let t: usize;
        asm!("mv {},sp",out(reg)t);
        t
    };
    let mut ans_str = Vec::new();
    loop {
        let first_ins = ins.read_volatile();
        trace!(
            "first_ins: {:#x} {:#b}",
            first_ins.get_bits(0..16),
            first_ins.get_bits(0..16)
        );
        let ans = InstructionSp::new(first_ins);
        let (next_ins, size) = match ans {
            InstructionSp::Addi(size) => {
                //四字节指令
                (ins.add(1).read_volatile(), size)
            }
            InstructionSp::CAddi(size) | InstructionSp::CAddi16Sp(size) => {
                // 双字节指令
                let ins = (ins as *const u16).add(1) as *const u32;
                (ins.read_volatile(), size)
            }
            InstructionSp::Unknown => {
                //未知指令
                break;
            }
        };
        //第二条指令就是记录有ra的值
        //需要确保第二条指令是否是存储ra
        // if let Some(val) = sd_ra(next_ins){
        //     info!("ra: {}",val);
        // }
        if sd_ra(next_ins).is_none() {
            break;
        }
        let stack_size = size;
        let ra_addr = sp + stack_size as usize - 8;
        let ra = (ra_addr as *const usize).read_volatile(); //8字节存储
        trace!("ra: {:#x}", ra);

        let mut flag = false;
        for i in 0..symbol.len() {
            if symbol[i].addr() == ra
                || (i + 1 < symbol.len() && (symbol[i].addr()..symbol[i + 1].addr()).contains(&ra))
            {
                let str = format!(
                    "{:#x} (+{}) {}",
                    symbol[i].addr(),
                    ra - symbol[i].addr(),
                    symbol[i].name()
                );
                trace!("{}", str);
                ins = symbol[i].addr() as *const u32;
                flag = true;
                ans_str.push(str.clone());
                break;
            }
        }
        if !flag {
            break;
        }
        sp += stack_size as usize;
    }
    ans_str
}

use super::unwinder::UnwinderError;
use gimli::{Register, RiscV};

#[derive(Debug, Default)]
pub struct RegisterSet {
    pc: Option<u64>,
    sp: Option<u64>,
    fp: Option<u64>,
    ra: Option<u64>,
}

/// The riscv64 machine state.
#[derive(Debug)]
pub struct MachineState {
    pub pc: u64,
    pub sp: u64,
    pub fp: u64,
    pub ra: u64,
}

impl MachineState {
    #[allow(dead_code)]
    pub fn new(pc: u64, sp: u64, fp: u64, ra: u64) -> Self {
        Self { pc, sp, fp, ra }
    }
}

impl RegisterSet {
    #[allow(dead_code)]
    pub fn from_machine_state(machine: &MachineState) -> Self {
        Self {
            pc: Some(machine.pc),
            sp: Some(machine.sp),
            fp: Some(machine.fp),
            ra: Some(machine.ra),
        }
    }

    pub fn get(&self, reg: Register) -> Option<u64> {
        match reg {
            RiscV::SP => self.sp,
            RiscV::S0 => self.fp,
            RiscV::RA => self.ra,
            _ => None,
        }
    }

    pub fn set(&mut self, reg: Register, val: u64) -> Result<(), UnwinderError> {
        *match reg {
            RiscV::SP => &mut self.sp,
            RiscV::S0 => &mut self.fp,
            RiscV::RA => &mut self.ra,
            _ => return Err(UnwinderError::UnexpectedRegister(reg)),
        } = Some(val);

        Ok(())
    }

    pub fn undef(&mut self, reg: Register) {
        *match reg {
            RiscV::SP => &mut self.sp,
            RiscV::S0 => &mut self.fp,
            RiscV::RA => &mut self.ra,
            _ => return,
        } = None;
    }

    pub fn get_pc(&self) -> Option<u64> {
        self.pc
    }

    pub fn set_pc(&mut self, val: u64) {
        self.pc = Some(val);
    }

    pub fn get_ret(&self) -> Option<u64> {
        self.ra
    }

    pub fn set_stack_ptr(&mut self, val: u64) {
        self.sp = Some(val);
    }

    pub fn iter() -> impl Iterator<Item = Register> {
        [RiscV::SP, RiscV::S0, RiscV::RA].into_iter()
    }
}

use super::arch::{MachineState, RegisterSet};
use super::{
    __kernel_eh_frame, __kernel_eh_frame_end, __kernel_eh_frame_hdr, __kernel_eh_frame_hdr_end,
};
use alloc::boxed::Box;
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use core::ptr::addr_of;
use core::slice;
use gimli::{
    BaseAddresses, CfaRule, EhFrame, EhFrameHdr, EhHdrTable, EndianSlice, LittleEndian,
    ParsedEhFrameHdr, Register, RegisterRule, UnwindContext, UnwindSection,
};
use log::trace;

#[derive(Debug)]
pub struct Backtrace {
    unwinder: Unwinder,
}

type VAddr = usize;

#[derive(Debug)]
pub struct CallFrame {
    pub pc: VAddr,
    pub symbol: Option<&'static str>,
    pub sym_off: Option<usize>,
    pub file_line: Option<(&'static str, u32)>,
}

impl Backtrace {
    pub fn from_machine_state(machine: &MachineState) -> Self {
        Self {
            unwinder: Unwinder::new(EhInfo::new(), RegisterSet::from_machine_state(machine)),
        }
    }
}

impl Iterator for Backtrace {
    type Item = CallFrame;

    fn next(&mut self) -> Option<Self::Item> {
        let pc = self.unwinder.next().ok()??;

        if pc == 0 {
            return None;
        }
        Some(CallFrame {
            pc: pc as usize,
            symbol: None,
            sym_off: None,
            file_line: None,
        })
    }
}

#[derive(Debug)]
pub enum UnwinderError {
    UnexpectedRegister(Register),
    UnsupportedCfaRule,
    CfaRuleUnknownRegister(Register),
    UnimplementedRegisterRule,
    NoUnwindInfo,
    NoPcRegister,
    NoReturnAddr,
}

#[derive(Debug)]
struct EhInfo {
    base_addrs: BaseAddresses,
    #[allow(dead_code)]
    hdr: &'static ParsedEhFrameHdr<EndianSlice<'static, LittleEndian>>,
    hdr_table: EhHdrTable<'static, EndianSlice<'static, LittleEndian>>,
    eh_frame: EhFrame<EndianSlice<'static, LittleEndian>>,
}

impl EhInfo {
    fn new() -> Self {
        let hdr = unsafe { addr_of!(__kernel_eh_frame_hdr) };
        let hdr_len = (unsafe { addr_of!(__kernel_eh_frame_hdr_end) } as usize) - (hdr as usize);
        let eh_frame = unsafe { addr_of!(__kernel_eh_frame) };
        let eh_frame_len =
            (unsafe { addr_of!(__kernel_eh_frame_end) } as usize) - (eh_frame as usize);
        trace!("hdr: {:p}, len: {}", hdr, hdr_len);
        trace!("eh_frame: {:p}, len: {}", eh_frame, eh_frame_len);
        let mut base_addrs = BaseAddresses::default();
        base_addrs = base_addrs.set_eh_frame_hdr(hdr as u64);

        let hdr = Box::leak(Box::new(
            EhFrameHdr::new(
                // TODO: remove Box
                unsafe { slice::from_raw_parts(hdr, hdr_len) },
                LittleEndian,
            )
            .parse(&base_addrs, size_of::<usize>() as u8)
            .unwrap(),
        ));

        base_addrs = base_addrs.set_eh_frame(eh_frame as u64);

        let eh_frame = EhFrame::new(
            unsafe { slice::from_raw_parts(eh_frame, eh_frame_len) },
            LittleEndian,
        );

        Self {
            base_addrs,
            hdr,
            hdr_table: hdr.table().unwrap(),
            eh_frame,
        }
    }
}

struct Unwinder {
    eh_info: EhInfo,
    unwind_ctx: UnwindContext<EndianSlice<'static, LittleEndian>>,
    regs: RegisterSet,
    cfa: u64,
    is_first: bool,
}

impl Debug for Unwinder {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Unwinder")
            .field("regs", &self.regs)
            .field("cfa", &self.cfa)
            .finish()
    }
}

impl Unwinder {
    fn new(eh_info: EhInfo, register_set: RegisterSet) -> Self {
        Self {
            eh_info,
            unwind_ctx: UnwindContext::new(), // TODO: no alloc
            regs: register_set,
            cfa: 0,
            is_first: true,
        }
    }

    fn next(&mut self) -> Result<Option<u64>, UnwinderError> {
        let pc = self.regs.get_pc().ok_or(UnwinderError::NoPcRegister)?;
        if self.is_first {
            self.is_first = false;
            return Ok(Some(pc));
        }
        let row = self
            .eh_info
            .hdr_table
            .unwind_info_for_address(
                &self.eh_info.eh_frame,
                &self.eh_info.base_addrs,
                &mut self.unwind_ctx,
                pc,
                |section, bases, offset| section.cie_from_offset(bases, offset),
            )
            .map_err(|_| UnwinderError::NoUnwindInfo)?;

        match row.cfa() {
            CfaRule::RegisterAndOffset { register, offset } => {
                let reg_val = self
                    .regs
                    .get(*register)
                    .ok_or(UnwinderError::CfaRuleUnknownRegister(*register))?;
                self.cfa = (reg_val as i64 + offset) as u64;
            }
            _ => return Err(UnwinderError::UnsupportedCfaRule),
        }

        // find the symbol associated with the current pc
        for reg in RegisterSet::iter() {
            match row.register(reg) {
                RegisterRule::Undefined => self.regs.undef(reg),
                RegisterRule::SameValue => (),
                RegisterRule::Offset(offset) => {
                    let ptr = (self.cfa as i64 + offset) as u64 as *const usize;
                    self.regs.set(reg, unsafe { ptr.read() } as u64)?;
                }
                RegisterRule::Register(_r) => {
                    todo!()
                }
                RegisterRule::ValOffset(offset) => {
                    let value = self.cfa as i64 + offset;
                    self.regs.set(reg, value as u64)?;
                }
                RegisterRule::Expression(_ex) => {
                    todo!()
                }
                RegisterRule::ValExpression(_ex) => {
                    todo!()
                }
                _ => return Err(UnwinderError::UnimplementedRegisterRule),
            }
        }
        let ret = self.regs.get_ret().ok_or(UnwinderError::NoReturnAddr)?;
        self.regs.set_pc(ret);
        self.regs.set_stack_ptr(self.cfa);
        Ok(Some(ret))
    }
}

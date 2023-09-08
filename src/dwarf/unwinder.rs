use super::arch::{MachineState, RegisterSet};
use crate::{DwarfProvider, TraceInfo, Tracer, TracerProvider};
use alloc::boxed::Box;
use core::arch::asm;
use core::fmt::{Debug, Formatter};
use core::mem::size_of;
use core::slice;
use gimli::{
    BaseAddresses, CfaRule, EhFrame, EhFrameHdr, EhHdrTable, EndianSlice, LittleEndian,
    ParsedEhFrameHdr, Register, RegisterRule, UnwindContext, UnwindSection,
};
use log::trace;
use crate::utils::read_value;

pub struct DwarfTracer<T, M> {
    dwarf_provider: T,
    machine_state: MachineState,
    tracer_provider: M,
}

impl<T: DwarfProvider, M: TracerProvider> DwarfTracer<T, M> {
    pub fn new(dwarf_provider: T, tracer_provider: M) -> Self {
        let machine = MachineState {
            pc: {
                let pc: usize;
                unsafe {
                    asm!("auipc {},0", out(reg) pc);
                }
                pc as u64
            },
            sp: {
                let sp: usize;
                unsafe {
                    asm!("mv {},sp", out(reg) sp);
                }
                sp as u64
            },
            fp: {
                let fp: usize;
                unsafe {
                    asm!("mv {},s0", out(reg) fp);
                }
                fp as u64
            },
            ra: {
                let ra: usize;
                unsafe {
                    asm!("mv {},ra", out(reg) ra);
                }
                ra as u64
            },
        };
        Self {
            machine_state: machine,
            dwarf_provider,
            tracer_provider,
        }
    }
}

struct DwarfTracerIterator<'a, M> {
    unwinder: Unwinder,
    provider: &'a M,
}

impl<T: DwarfProvider, M: TracerProvider> Tracer for DwarfTracer<T, M> {
    fn trace(&self) -> impl Iterator<Item = TraceInfo> + '_ {
        let unwinder = Unwinder::new(
            EhInfo::new(&self.dwarf_provider),
            RegisterSet::from_machine_state(&self.machine_state),
        );
        DwarfTracerIterator {
            unwinder,
            provider: &self.tracer_provider,
        }
    }
}

impl<M: TracerProvider> Iterator for DwarfTracerIterator<'_, M> {
    type Item = TraceInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let pc = self.unwinder.next().ok()??;

        if pc == 0 {
            return None;
        }
        let info = self.provider.address2symbol(pc as usize)?;
        Some(TraceInfo {
            func_name: info.1,
            func_addr: info.0,
            bias: pc as usize - info.0,
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
    #[allow(unused)]
    hdr: &'static ParsedEhFrameHdr<EndianSlice<'static, LittleEndian>>,
    hdr_table: EhHdrTable<'static, EndianSlice<'static, LittleEndian>>,
    eh_frame: EhFrame<EndianSlice<'static, LittleEndian>>,
}

impl EhInfo {
    fn new<T: DwarfProvider>(provider: &T) -> Self {
        let hdr = provider.kernel_eh_frame_hdr();
        let hdr_len = provider.kernel_eh_frame_hdr_end() - hdr;
        let eh_frame = provider.kernel_eh_frame();
        let eh_frame_len = provider.kernel_eh_frame_end() - eh_frame;
        trace!("hdr: {:#x?}, len: {}", hdr, hdr_len);
        trace!("eh_frame: {:#x?}, len: {}", eh_frame, eh_frame_len);
        let mut base_addrs = BaseAddresses::default();
        base_addrs = base_addrs.set_eh_frame_hdr(hdr as u64);

        let hdr = Box::leak(Box::new(
            EhFrameHdr::new(
                // TODO: remove Box
                unsafe { slice::from_raw_parts(hdr as *const u8, hdr_len) },
                LittleEndian,
            )
            .parse(&base_addrs, size_of::<usize>() as u8)
            .unwrap(),
        ));
        base_addrs = base_addrs.set_eh_frame(eh_frame as u64);
        let eh_frame = EhFrame::new(
            unsafe { slice::from_raw_parts(eh_frame as *const u8, eh_frame_len) },
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

        trace!("row: {:#x?}", row);
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
        trace!("cfa:{:#x}, regs:{:#x?}", self.cfa, self.regs);

        // find the symbol associated with the current pc
        for reg in RegisterSet::iter() {
            let rule = row.register(reg);
            trace!("reg: {:?}, rule: {:?}", reg, rule);
            match rule {
                RegisterRule::Undefined => self.regs.undef(reg),
                RegisterRule::SameValue => (),
                RegisterRule::Offset(offset) => {
                    let ptr = (self.cfa as i64 + offset) as usize;
                    self.regs.set(reg, read_value(ptr) as u64)?;
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
        trace!("after cal, regs:{:#x?}", self.regs);
        let ret = self.regs.get_ret().ok_or(UnwinderError::NoReturnAddr)?;
        self.regs.set_pc(ret);
        self.regs.set_stack_ptr(self.cfa);
        Ok(Some(ret))
    }
}

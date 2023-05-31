mod arch;
mod expression;
mod unwinder;

extern "C" {
    /// The user should define these symbols in their linker script.
    static __kernel_eh_frame_hdr: u8;
    static __kernel_eh_frame_hdr_end: u8;
    static __kernel_eh_frame: u8;
    static __kernel_eh_frame_end: u8;
}

pub use arch::MachineState;
pub use unwinder::Backtrace;

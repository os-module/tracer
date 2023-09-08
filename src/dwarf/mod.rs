mod arch;
mod expression;
mod unwinder;

pub use unwinder::DwarfTracer;

/// The user should define these symbols in their linker script.
pub trait DwarfProvider {
    fn kernel_eh_frame_hdr(&self) -> usize;
    fn kernel_eh_frame(&self) -> usize;
    fn kernel_eh_frame_hdr_end(&self) -> usize;
    fn kernel_eh_frame_end(&self) -> usize;
}

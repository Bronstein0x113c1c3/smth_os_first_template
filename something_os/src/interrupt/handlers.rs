// use crate::{gdt, hlt_loop, print, println};

#[macro_use]
use crate::vga_buffer;
use crate::{print,println};
use pic8259::ChainedPics;
use spin;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use crate::process;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Yield = 0x30,
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

pub static PICS: spin::Mutex<ChainedPics> =
spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });


pub extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt_loop();
}

pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

// pub extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
//     // println!("timer called");
//     unsafe {
//         PICS.lock()
//         .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
//     }
//
//     if let Some(sched_lock) = crate::process::SCHEDULER.try_lock() {
//         if sched_lock.is_some() && sched_lock.as_ref().unwrap().processes.len() >= 2 {
//             // Drop the lock guard explicitly before triggering the yield
//             core::mem::drop(sched_lock);
//             crate::process::yield_now();
//         }
//     }
// }


// --- 1. The Hardware Timer Interrupt ---
pub const YIELD_INTERRUPT_VECTOR: u8 = 0x30;

#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    core::arch::naked_asm!(
        // 1. Save all general purpose registers onto the current process stack
        "push rbp", "push r15", "push r14", "push r13", "push r12",
        "push r11", "push r10", "push r9", "push r8", "push rdi",
        "push rsi", "push rdx", "push rcx", "push rbx", "push rax",

        // 2. Clear the hardware interrupt controller (Send EOI)
        "call {signal_eoi}",

        // 3. Pass the current stack pointer (RSP) as the first argument to our router
        "mov rdi, rsp",
        "call {preempt_round_robin}",

        // 4. The scheduler returns the NEW process's stack pointer in RAX. Load it.
        "mov rsp, rax",

        // 5. Restore registers for the newly chosen process
        "pop rax", "pop rbx", "pop rcx", "pop rdx", "pop rsi",
        "pop rdi", "pop r8", "pop r9", "pop r10", "pop r11",
        "pop r12", "pop r13", "pop r14", "pop r15", "pop rbp",

        // 6. Flawlessly resume execution using the hardware interrupt return sequence
        "iretq",
        signal_eoi = sym signal_eoi,
        preempt_round_robin = sym preempt_round_robin,
    );
}

/// Helper function to clear the PIC hardware channels
extern "C" fn signal_eoi() {
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

/// Helper function to step the scheduler forward
#[unsafe(no_mangle)]
extern "C" fn preempt_round_robin(old_rsp: u64) -> u64 {
    if let Some(mut sched_lock) = crate::process::SCHEDULER.try_lock() {
        if let Some(ref mut sched) = *sched_lock {
            if sched.processes.len() < 2 { return old_rsp; }

            // Save old RSP directly into the active context tracking structure
            let old_index = sched.current_index;
            sched.processes[old_index].context.rsp = old_rsp;
            sched.processes[old_index].state = crate::process::ProcessState::Ready;

            // Advance index to the next thread
            let next_index = (old_index + 1) % sched.processes.len();
            sched.current_index = next_index;
            sched.processes[next_index].state = crate::process::ProcessState::Running;

            // Return the target stack pointer to the assembly wrapper
            return sched.processes[next_index].context.rsp;
        }
    }
    old_rsp
}
/// This handler catches 'int 0x30'. It forces the CPU to exit the hardware
/// interrupt context and invokes your working cooperative yield safely.
#[unsafe(naked)]
pub extern "x86-interrupt" fn software_yield_handler(_stack_frame: InterruptStackFrame) {
    core::arch::naked_asm!(
        // The CPU pushed an interrupt frame. We don't want it messing up our
        // cooperative scheduler stack calculations, so we call a Rust stub.
        "call {preempt_bridge}",
        "iretq",
        preempt_bridge = sym preempt_bridge,
    );
}

/// Bridge function that runs outside the raw interrupt frame constraints
extern "C" fn preempt_bridge() {
    // Simply call your existing yield logic!
    crate::process::yield_now();
}

#[unsafe(naked)]
pub extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        core::arch::naked_asm!(
            // A. Save all general-purpose registers onto the current process stack
            "push rbp", "push r15", "push r14", "push r13", "push r12",
            "push r11", "push r10", "push r9", "push r8", "push rdi",
            "push rsi", "push rdx", "push rcx", "push rbx", "push rax",

            // B. Call a normal Rust helper to read scancode, print, and send EOI
            "call {handle_keyboard_io_and_eoi}",

            // C. Pass the current stack pointer (RSP) to the scheduler router
            "mov rdi, rsp",
            "call {preempt_round_robin}",

            // D. The scheduler returns the NEW process's stack pointer in RAX. Load it.
            "mov rsp, rax",

            // E. Restore registers for the newly chosen process
            "pop rax", "pop rbx", "pop rcx", "pop rdx", "pop rsi",
            "pop rdi", "pop r8", "pop r9", "pop r10", "pop r11",
            "pop r12", "pop r13", "pop r14", "pop r15", "pop rbp",

            // F. Hardware interrupt return sequence
            "iretq",
            handle_keyboard_io_and_eoi = sym handle_keyboard_io_and_eoi,
            preempt_round_robin = sym preempt_round_robin,
        );
    }
}

// 2. The standard Rust helper manages I/O safely after registers are preserved
extern "C" fn handle_keyboard_io_and_eoi() {
    use x86_64::instructions::port::Port;

    // A. Read scancode from PS/2 Controller
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // B. Safely print the trace statement
    println!("Keyboard pressed! Scancode: {}. Switching process...", scancode);

    // C. Clear the hardware interrupt controller queue
    unsafe {
        PICS.lock()
        .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

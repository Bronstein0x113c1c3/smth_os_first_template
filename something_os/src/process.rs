// src/process.rs

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use spin::Mutex;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    registers::control::Cr3,
    VirtAddr,
};

pub static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcessContext {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64, // Offset 64 <-- ADD THIS FIELD HERE
}

impl ProcessContext {
    pub const fn zero() -> Self {
        Self {
            r15: 0, r14: 0, r13: 0, r12: 0, rbx: 0, rbp: 0,
            rip: 0, rsp: 0, rflags: 0,
        }
    }
}

pub struct Process {
    pub id: u64,
    pub state: ProcessState,
    pub context: ProcessContext,
    pub _stack: Box<[u8]>,
}

pub struct Scheduler {
    pub processes: VecDeque<Process>,
    pub current_index: usize,
}

impl Scheduler {
    pub fn init(_physical_memory_offset: VirtAddr) {
        let mut sched = SCHEDULER.lock();
        *sched = Some(Scheduler {
            processes: VecDeque::new(),
                      current_index: 0,
        });
    }

    pub fn spawn_root(&mut self) {
        let root_process = Process {
            id: 0,
            state: ProcessState::Running,
            context: ProcessContext::zero(),
            _stack: Box::new([]),
        };
        self.processes.push_back(root_process);
        self.current_index = 0;
    }

    pub fn spawn<A>(&mut self, entry_point: fn() -> !, _frame_allocator: &mut A) -> u64
    where
    A: FrameAllocator<Size4KiB>,
    {
        static NEXT_PID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
        let pid = NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::SeqCst);

        const STACK_SIZE: usize = 4096 * 4;
        let mut stack = Box::new([0u8; STACK_SIZE]);

        let stack_top = unsafe {
            let mut ptr = stack.as_mut_ptr().add(STACK_SIZE) as *mut u64;

            // --- Forge the Hardware Interrupt Frame (Required by iretq) ---
            ptr = ptr.sub(1); *ptr = 0x10;               // SS (Data Segment Selector)
            ptr = ptr.sub(1); *ptr = ptr as u64 + 16;    // RSP (Stack pointer pointing to top)
            ptr = ptr.sub(1); *ptr = 0x200;              // RFLAGS (0x200 keeps interrupts enabled!)
            ptr = ptr.sub(1); *ptr = 0x08;               // CS (Code Segment Selector)
            ptr = ptr.sub(1); *ptr = entry_point as u64; // RIP (Where execution begins)

            // --- Forge the 15 General Purpose Registers (Required by assembly pops) ---
            for _ in 0..15 {
                ptr = ptr.sub(1);
                *ptr = 0; // Initialize general purpose registers to 0
            }

            ptr as u64
        };

        let mut context = ProcessContext::zero();
        context.rsp = stack_top; // Set context structure directly to our aligned top pointer

        let process = Process {
            id: pid,
            state: ProcessState::Ready,
            context,
            _stack: stack,
        };

        self.processes.push_back(process);
        pid
    }

    // Inside src/process.rs -> impl Scheduler

    // pub fn switch(&mut self) {
    //     if self.processes.len() < 2 {
    //         return;
    //     }
    //
    //     let old_index = self.current_index;
    //     let next_index = (old_index + 1) % self.processes.len();
    //     self.current_index = next_index;
    //
    //     self.processes[old_index].state = ProcessState::Ready;
    //     self.processes[next_index].state = ProcessState::Running;
    //
    //     // FIX: Instead of getting safe Rust references that conflict,
    //     // we obtain direct raw pointers to the fields using the underlying buffer address.
    //     unsafe {
    //         // 1. Get raw pointers directly from the indexing syntax to bypass the borrow checker
    //         let old_ctx_ptr = &mut self.processes[old_index].context as *mut ProcessContext;
    //         let new_ctx_ptr = &self.processes[next_index].context as *const ProcessContext;
    //
    //         // 2. Safely perform the low-level assembly register swap
    //         context_switch(old_ctx_ptr, new_ctx_ptr);
    //     }
    // }

    pub fn switch(&mut self) {
        let mut old_ptr: *mut ProcessContext = core::ptr::null_mut();
        let mut new_ptr: *const ProcessContext = core::ptr::null();

        if let Some(ref mut sched) = *SCHEDULER.lock() {
            if sched.processes.len() < 2 { return; }

            let old_index = sched.current_index;
            let next_index = (old_index + 1) % sched.processes.len();
            sched.current_index = next_index;

            sched.processes[old_index].state = ProcessState::Ready;
            sched.processes[next_index].state = ProcessState::Running;

            old_ptr = &mut sched.processes[old_index].context as *mut ProcessContext;
            new_ptr = &sched.processes[next_index].context as *const ProcessContext;
        } // <--- The SCHEDULER Mutex lock guard cleanly drops right here

        // Perform the low-level context switch outside the spinlock scope
        if !old_ptr.is_null() && !new_ptr.is_null() {
            unsafe {
                context_switch(old_ptr, new_ptr);
            }
        }
    }
}

core::arch::global_asm!(
    r#"
    .global context_switch
    .type context_switch, @function
    context_switch:
    # rdi -> pointer to old ProcessContext
    # rsi -> pointer to new ProcessContext

    # 1. SAVE OLD TASK CONTEXT
    mov [rdi + 0], r15
    mov [rdi + 8], r14
    mov [rdi + 16], r13
    mov [rdi + 24], r12
    mov [rdi + 32], rbx
    mov [rdi + 40], rbp
    mov [rdi + 56], rsp # Save stack pointer

    mov rax, [rsp]      # Get return address from stack
    mov [rdi + 48], rax # Save as RIP

    # 2. LOAD NEW TASK CONTEXT
    mov r15, [rsi + 0]
    mov r14, [rsi + 8]
    mov r13, [rsi + 16]
    mov r12, [rsi + 24]
    mov rbx, [rsi + 32]
    mov rbp, [rsi + 40]
    mov rsp, [rsi + 56] # Switch to new process stack pointer

    mov rax, [rsi + 48] # Get target RIP
    mov [rsp], rax      # Place it on top of the new stack

    # DO NOT use popfq here! We let the execution loop handle flags.
    ret
    "#
);

unsafe extern "C" {
    fn context_switch(old: *mut ProcessContext, new: *const ProcessContext);
}

// pub fn yield_now() {
//     x86_64::instructions::interrupts::without_interrupts(|| {
//         if let Some(ref mut sched) = *SCHEDULER.lock() {
//             sched.switch();
//         }
//     });
// }

// Inside src/process.rs

/// Manual, cooperative yield used voluntarily by processes
pub fn yield_now() {
    unsafe {
        // Disable interrupts during the critical scheduler recalculation phase
        x86_64::instructions::interrupts::disable();

        execute_switch();

        // Turn interrupts back on when this process eventually wakes back up
        x86_64::instructions::interrupts::enable();
    }
}

/// Forced, preemptive yield called exclusively by the hardware timer interrupt handler
pub fn preempt_now() {
    unsafe {
        // Hardware timer context requires interrupts to be disabled during the stack swap
        x86_64::instructions::interrupts::disable();

        execute_switch();

        // Essential: Re-enable interrupts when the newly scheduled process wakes up here!
        x86_64::instructions::interrupts::enable();
    }
}

/// Core scheduling wrapper that safely extracts pointers and fires the assembly swapper
fn execute_switch() {
    let mut old_ptr: *mut ProcessContext = core::ptr::null_mut();
    let mut new_ptr: *const ProcessContext = core::ptr::null();

    if let Some(ref mut sched) = *SCHEDULER.lock() {
        if sched.processes.len() < 2 { return; }

        let old_index = sched.current_index;
        let next_index = (old_index + 1) % sched.processes.len();
        sched.current_index = next_index;

        sched.processes[old_index].state = ProcessState::Ready;
        sched.processes[next_index].state = ProcessState::Running;

        old_ptr = &mut sched.processes[old_index].context as *mut ProcessContext;
        new_ptr = &sched.processes[next_index].context as *const ProcessContext;
    } // <--- The SCHEDULER Mutex lock guard cleanly drops right here

    // Perform the low-level context switch outside the spinlock scope
    if !old_ptr.is_null() && !new_ptr.is_null() {
        unsafe {
            context_switch(old_ptr, new_ptr);
        }
    }
}

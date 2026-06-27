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
}

impl ProcessContext {
    pub const fn zero() -> Self {
        Self {
            r15: 0, r14: 0, r13: 0, r12: 0, rbx: 0, rbp: 0,
            rip: 0, rsp: 0,
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
    processes: VecDeque<Process>,
    current_index: usize,
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

        // FIX 1: Adjust the stack top down by 8 bytes to make a safe slot
        // for the assembly `ret` instruction to pop from!
        let stack_top = (stack.as_ptr() as u64 + STACK_SIZE as u64) - 8;

        let mut context = ProcessContext::zero();
        context.rip = entry_point as u64;
        context.rsp = stack_top;

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
        if self.processes.len() < 2 {
            return;
        }

        let old_index = self.current_index;
        let next_index = (old_index + 1) % self.processes.len();
        self.current_index = next_index;

        self.processes[old_index].state = ProcessState::Ready;
        self.processes[next_index].state = ProcessState::Running;

        // 1. Extract raw pointers while the scheduler structure is accessible
        let old_ctx_ptr = &mut self.processes[old_index].context as *mut ProcessContext;
        let new_ctx_ptr = &self.processes[next_index].context as *const ProcessContext;

        // 2. CRITICAL FIX: We must temporarily move the context switch outside
        // the global Mutex lock state, or drop the lock explicitly if called from yield_now.
        unsafe {
            context_switch(old_ctx_ptr, new_ctx_ptr);
        }
    }
}

core::arch::global_asm!(
    r#"
    .global context_switch
    .type context_switch, @function
    context_switch:
    # 1. Save old task context
    mov [rdi + 0], r15
    mov [rdi + 8], r14
    mov [rdi + 16], r13
    mov [rdi + 24], r12
    mov [rdi + 32], rbx
    mov [rdi + 40], rbp
    mov [rdi + 56], rsp

    mov rax, [rsp]
    mov [rdi + 48], rax

    # 2. Load new task context
    mov r15, [rsi + 0]
    mov r14, [rsi + 8]
    mov r13, [rsi + 16]
    mov r12, [rsi + 24]
    mov rbx, [rsi + 32]
    mov rbp, [rsi + 40]
    mov rsp, [rsi + 56]

    mov rax, [rsi + 48]
    mov [rsp], rax

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

pub fn yield_now() {
    let mut old_ptr: *mut ProcessContext = core::ptr::null_mut();
    let mut new_ptr: *const ProcessContext = core::ptr::null();

    x86_64::instructions::interrupts::without_interrupts(|| {
        if let Some(ref mut sched) = *SCHEDULER.lock() {
            if sched.processes.len() < 2 { return; }

            let old_index = sched.current_index;
            let next_index = (old_index + 1) % sched.processes.len();
            sched.current_index = next_index;

            sched.processes[old_index].state = ProcessState::Ready;
            sched.processes[next_index].state = ProcessState::Running;

            old_ptr = &mut sched.processes[old_index].context as *mut ProcessContext;
            new_ptr = &sched.processes[next_index].context as *const ProcessContext;
        } // <--- The SCHEDULER Mutex is officially dropped and unlocked HERE!

        // Now it is perfectly safe to switch tasks because the lock is wide open
        if !old_ptr.is_null() && !new_ptr.is_null() {
            unsafe {
                context_switch(old_ptr, new_ptr);
            }
        }
    });
}

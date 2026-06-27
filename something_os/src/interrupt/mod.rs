pub mod handlers;
use crate::gdt;
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(handlers::breakpoint_handler);
        idt.page_fault.set_handler_fn(handlers::page_fault_handler);
        unsafe {
            idt.double_fault
            .set_handler_fn(handlers::double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt[handlers::InterruptIndex::Timer.as_usize()].set_handler_fn(handlers::timer_interrupt_handler);
        idt[handlers::InterruptIndex::Keyboard.as_usize()].set_handler_fn(handlers::keyboard_interrupt_handler);
        idt[handlers::InterruptIndex::Yield.as_usize()].set_handler_fn(handlers::software_yield_handler);
        idt
    };
}

// Inside a hardware initialization module
pub fn init_pit() {
    use x86_64::instructions::port::Port;

    // Command port: 0x43, Channel 0 port: 0x40
    let mut cmd_port = Port::<u8>::new(0x43);
    let mut data_port = Port::<u8>::new(0x40);

    unsafe {
        // Set PIT to square wave mode, repeating
        cmd_port.write(0x36);

        // Set frequency divider for roughly 100Hz (1193182 / 100)
        let divisor: u16 = 11931;
        data_port.write((divisor & 0xFF) as u8);        // Low byte
        data_port.write((divisor >> 8) as u8);         // High byte
    }
}


pub fn init_idt() {


    IDT.load();
    unsafe { handlers::PICS.lock().initialize() };
    init_pit();
    x86_64::instructions::interrupts::enable();
}

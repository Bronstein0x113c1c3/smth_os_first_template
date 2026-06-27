#![no_std]
#![feature(abi_x86_interrupt)]
#![no_main]
#[deny(unconditional_panic)]
#[macro_use]
pub mod vga_buffer;
pub mod gdt;
pub(crate) mod interrupt;
pub(crate) mod memory;
pub(crate) mod allocator;
pub(crate) mod process;

extern crate alloc;

use memory::BootInfoFrameAllocator;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use crate::interrupt::init_idt;
// use vga_buffer::print;

entry_point!(kernel_main);
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    gdt::init();
    init_idt();
    check_network_card();
    x86_64::instructions::interrupts::int3();
    // let i = 1;
    // print!("something {}", 5/i);

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

/*
    println!("Initializing Process Scheduler...");
    // 2. Initialize the global Scheduler
    process::Scheduler::init(phys_mem_offset);*/

    // 3. Register the current execution context as the base root process
    // so we have an 'old' space to save registers into when switching out.
    println!("Initializing Process Scheduler...");
    process::Scheduler::init(phys_mem_offset);

    if let Some(ref mut sched) = *process::SCHEDULER.lock() {
        // 1. FIX: Register the current execution state as the root process.
        sched.spawn_root();

        // 2. Spawn your actual workloads
        sched.spawn(process_alpha, &mut frame_allocator);
        sched.spawn(process_beta, &mut frame_allocator);
        sched.spawn(process_omega, &mut frame_allocator);
    }

    println!("Starting multi-process testing execution loop!\n");

    // 3. Now this call safely saves the real boot registers into PID 0
    // and seamlessly transitions execution to Process Alpha (PID 1).
    loop {
        // process::yield_now();
         x86_64::instructions::hlt();
    }



    // loop {
    //      x86_64::instructions::hlt();
    // }
}



/// Simple dummy workload representation
fn dummy_main_process() -> ! {
    loop {
        process::yield_now();
    }
}
pub fn process_alpha() -> ! {
    loop {
        x86_64::instructions::interrupts::without_interrupts(|| {
            print!("A");
        });

        // Give up a brief processing window where preemption can safely happen
        for _ in 0..50000 { core::hint::spin_loop(); }
    }
}

pub fn process_beta() -> ! {
    loop {
        x86_64::instructions::interrupts::without_interrupts(|| {
            print!("B");
        });

        for _ in 0..50000 { core::hint::spin_loop(); }
    }
}

pub fn process_omega() -> ! {
    loop {
        x86_64::instructions::interrupts::without_interrupts(|| {
            print!("C");
        });

        for _ in 0..50000 { core::hint::spin_loop(); }
    }
}




/// This function is called on panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

fn check_network_card() {
    use x86_64::instructions::port::Port;
    let mut address_port = Port::<u32>::new(0xCF8);
    let mut data_port = Port::<u32>::new(0xCFC);

    println!("Scanning PCI buses for Network Cards...");

    // Quét qua các Bus (0-255) và Slot thiết bị (0-31)
    for bus in 0..255 {
        for slot in 0..32 {
            // Cấu hình địa chỉ để đọc thông tin Vendor ID và Device ID (Offset 0)
            let address = (1 << 31) | (bus << 16) | (slot << 11) | 0x00;

            unsafe {
                address_port.write(address);
                let id_data = data_port.read();

                let vendor_id = (id_data & 0xFFFF) as u16;
                let device_id = (id_data >> 16) as u16;

                // Nếu Vendor ID khác 0xFFFF nghĩa là có thiết bị tồn tại ở slot này
                if vendor_id != 0xFFFF {
                    // Kiểm tra xem có trùng với Vendor ID của các card mạng phổ biến không
                    if vendor_id == 0x10EC && device_id == 0x8139 {
                        println!("Success: Found Realtek RTL8139 Network Card at Bus {}, Slot {}", bus, slot);
                    } else if vendor_id == 0x8086 && (device_id == 0x100E || device_id == 0x100F) {
                        println!("Success: Found Intel e1000 Network Card at Bus {}, Slot {}", bus, slot);
                    } else {
                        // In ra các thiết bị PCI khác nếu bạn muốn xem (Ví dụ: VGA, IDE Controller...)
                        // println!("Found PCI Device: Vendor {:#X}, Device {:#X}", vendor_id, device_id);
                    }
                }
            }
        }
    }
}


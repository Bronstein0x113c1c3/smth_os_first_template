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
pub mod networking;
use memory::BootInfoFrameAllocator;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use crate::interrupt::init_idt;
use spin::Mutex;
use x86_64::structures::paging::OffsetPageTable;
use bootloader::bootinfo::MemoryMap;
use crate::networking::RTL8139;
// use vga_buffer::print;

entry_point!(kernel_main);


// Tạo static Mutex để chia sẻ toàn cục
pub static MAPPER: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);
pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);
/// Hàm lấy địa chỉ vật lý u32 từ một tham chiếu mảng Rust
pub unsafe fn virt_to_phys_u32(mapper: &OffsetPageTable, vaddr_ptr: *const u8) -> u32 {
    use x86_64::structures::paging::Translate;
    let virt_addr = VirtAddr::new(vaddr_ptr as u64);
    // Tra cứu bảng phân trang PML4 -> PDPT -> PD -> PT để tìm địa chỉ vật lý thật
    match mapper.translate_addr(virt_addr) {
        Some(phys_addr) => phys_addr.as_u64() as u32,
        None => panic!("RTL8139 Error: Failed to translate buffer virtual address to physical!"),
    }
}
// Sửa lại hàm init trong memory.rs của bạn để khởi tạo các biến static này
pub unsafe fn init_global_memory(phys_mem_offset: VirtAddr, memory_map: &'static MemoryMap) {
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(memory_map) };

    // Đưa vào global static
    *MAPPER.lock() = Some(mapper);
    *FRAME_ALLOCATOR.lock() = Some(frame_allocator);
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    gdt::init();
    init_idt();
    x86_64::instructions::interrupts::int3();
    // let i = 1;
    // print!("something {}", 5/i);

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    // let mut mapper = unsafe { memory::init(phys_mem_offset) };
    // let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    unsafe {
                init_global_memory(phys_mem_offset,&boot_info.memory_map );
    }

    // allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");
    allocator::init_heap();

    networking::check_network_card();

    // Đoạn code test đặt sau khi đã khởi tạo PCI/Driver thành công:
    if let Some(ref mut driver) = *RTL8139.lock() {
        // Tạo gói tin thô (6 bytes MAC đích, 6 bytes MAC nguồn, 2 bytes Type, dữ liệu...)
        let mut test_packet = [0u8; 60];

        // MAC Đích: FF:FF:FF:FF:FF:FF (Broadcast gửi tới toàn mạng)
        test_packet[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

        // MAC Nguồn: Đọc từ card hoặc điền đại một giá trị giả lập (Ví dụ: 52:54:00:12:34:56)
        test_packet[6..12].copy_from_slice(&[0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);

        // EtherType: 0x0800 (IPv4) hoặc 0x0806 (ARP) - Ở đây dùng đại 0x0000 để test payload thô
        test_packet[12..14].copy_from_slice(&[0x00, 0x00]);

        // Payload dữ liệu (Ví dụ: Chữ "Hello from Rust OS!")
        let msg = b"Hello from Rust OS!";
        test_packet[14..14 + msg.len()].copy_from_slice(msg);

        unsafe {
            // Lấy mapper toàn cục của OS của bạn (tên biến tùy thuộc vào code của bạn, ví dụ: MAPPER)
            let mapper = MAPPER.lock();

            // Dùng .as_ref() để unwrap ra một tham chiếu (&OffsetPageTable) thay vì move giá trị
            if let Some(ref mapper) = mapper.as_ref() {

                // 1. Dịch địa chỉ ảo thành địa chỉ vật lý
                let packet_phys_addr = virt_to_phys_u32(mapper, test_packet.as_ptr());
                println!("Virtual Addr: {:p} -> Physical Addr: {:#X}", test_packet.as_ptr(), packet_phys_addr);

                // 2. Gửi địa chỉ vật lý thật cho card mạng
                driver.send_packet(packet_phys_addr, test_packet.len() as u32);

            } else {
                println!("Error: KERNEL_MAPPER is None!");
            }
        }
    }


/*
    println!("Initializing Process Scheduler...");
    // 2. Initialize the global Scheduler
    process::Scheduler::init(phys_mem_offset);*/

    // 3. Register the current execution context as the base root process
    // so we have an 'old' space to save registers into when switching out.
    // println!("Initializing Process Scheduler...");
    // process::Scheduler::init(phys_mem_offset);
    //
    // if let Some(ref mut sched) = *process::SCHEDULER.lock() {
    //     // 1. FIX: Register the current execution state as the root process.
    //     sched.spawn_root();
    //
    //     // 2. Spawn your actual workloads
    //     sched.spawn(process_alpha);
    //     sched.spawn(process_beta);
    //     sched.spawn(process_omega);
    // }
    //
    // println!("Starting multi-process testing execution loop!\n");

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



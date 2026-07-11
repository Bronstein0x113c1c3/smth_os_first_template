pub mod rtl8139;
use spin::Mutex;
use crate::networking::rtl8139::Rtl8139Driver;
pub static RTL8139: Mutex<Option<rtl8139::Rtl8139Driver>> = Mutex::new(None);
pub fn check_network_card() {
    use crate::println;

    use x86_64::instructions::port::Port;
    let mut address_port = Port::<u32>::new(0xCF8);
    let mut data_port = Port::<u32>::new(0xCFC);

    println!("Scanning PCI buses for Network Cards...");

    // Quét qua các Bus (0-255) và Slot thiết bị (0-31)
    for bus in 0..255 {
        for slot in 0..32 {
            let func = 0;
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

                            Rtl8139Driver::init_device(bus, slot, func, &mut address_port, &mut data_port);


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

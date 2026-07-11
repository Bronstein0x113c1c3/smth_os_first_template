use x86_64::instructions::port::Port;
use crate::println;
use core::sync::atomic::{compiler_fence, Ordering};
use crate::networking::RTL8139;
/// Kích thước bộ đệm nhận: 8KB + 16 bytes tiêu đề vòng lặp (ring buffer overhead)
const RX_BUFFER_SIZE: usize = 8192 + 16;

pub struct Rtl8139Driver {
    io_base: u16,
    // Bộ đệm nhận dữ liệu. Cần được căn lề (align) để tránh lỗi phần cứng.
    // Thực tế trong OS lớn, vùng này nên được cấp phát động (page allocation).
    rx_buffer: [u8; RX_BUFFER_SIZE],
    tx_index: u8, // Bắt đầu từ cổng số 0
}

impl Rtl8139Driver {
    /// Tạo một instance mới cho Driver
    pub fn new(io_base: u16) -> Self {
        Self {
            io_base,
            rx_buffer: [0; RX_BUFFER_SIZE],
            tx_index: 0, // Bắt đầu từ cổng số 0
        }
    }
    /// Hàm gửi một gói tin thô (raw ethernet packet) ra mạng
    pub unsafe fn send_packet(&mut self, phys_addr: u32, length: u32) {
        let tsad_offset = 0x20 + (self.tx_index as u16 * 4);
        let tsd_offset  = 0x10 + (self.tx_index as u16 * 4);

        let mut port_tsad = Port::<u32>::new(self.io_base + tsad_offset);
        let mut port_tsd  = Port::<u32>::new(self.io_base + tsd_offset);

        // Ghi địa chỉ VẬT LÝ chuẩn xác vào thanh ghi
        port_tsad.write(phys_addr);

        // Ghi độ dài để ra lệnh kích hoạt DMA phát gói tin
        port_tsd.write(length & 0x1FFF);

        // Chờ chip mạng DMA xong và gửi thành công (Bit 13 chuyển thành 1)
        while (port_tsd.read() & (1 << 13)) == 0 {
            core::hint::spin_loop();
        }

        self.tx_index = (self.tx_index + 1) % 4;
        println!("RTL8139: Real packet data successfully pushed over DMA!");
    }
    pub unsafe fn init_device(
        bus: u32,
        slot: u32,
        func: u32,
        address_port: &mut Port<u32>,
        data_port: &mut Port<u32>
    ) {
        // 1. Bật Bus Mastering & I/O Space trên PCI Bus
        let cmd_address = (1 << 31) | (bus << 16) | (slot << 11) | (func << 8) | 0x04;
        address_port.write(cmd_address);
        let mut cmd_status = data_port.read();
        let command = ((cmd_status & 0xFFFF) as u16) | (1 << 2) | (1 << 0);
        data_port.write((cmd_status & 0xFFFF_0000) | (command as u32));

        // 2. Đọc BAR0 để lấy I/O Base Address
        let bar0_address = (1 << 31) | (bus << 16) | (slot << 11) | (func << 8) | 0x10;
        address_port.write(bar0_address);
        let bar0 = data_port.read();

        // 3. Nếu là I/O Mapping, tiến hành khởi tạo driver
        if (bar0 & 1) == 1 {
            let io_base = (bar0 & !0x3) as u16;
            println!("-> RTL8139 IO Base Address: {:#X}", io_base);

            // Tạo thực thể mới và gọi hàm setup phần cứng nội bộ
            let mut driver = Self::new(io_base);
            driver.init(); // Gọi hàm init() cấu hình thanh ghi gạt bit đã viết ở trước

            // Đẩy thẳng vào biến static toàn cục
            *RTL8139.lock() = Some(driver);
            println!("-> RTL8139 Driver initialized successfully!");
        } else {
            println!("-> Error: RTL8139 is using Memory Mapping, expected I/O Mapping.");
        }
    }

    /// Hàm khởi tạo cấu hình phần cứng RTL8139
    pub unsafe fn init(&mut self) {
        // Định nghĩa các cổng I/O dựa trên io_base và offset của thanh ghi
        let mut port_cr       = Port::<u8>::new(self.io_base + 0x37);  // Command Register
        let mut port_rbstart  = Port::<u32>::new(self.io_base + 0x30); // Receive Buffer Start
        let mut port_imr      = Port::<u16>::new(self.io_base + 0x3C); // Interrupt Mask Register
        let mut port_isr      = Port::<u16>::new(self.io_base + 0x3E); // Interrupt Status Register
        let mut port_rcr      = Port::<u32>::new(self.io_base + 0x44); // Receive Configuration Register

        println!("RTL8139: Starting hardware initialization...");

        // -------------------------------------------------------------------------
        // Bước 1: Software Reset
        // -------------------------------------------------------------------------
        // Ghi bit RST (0x10) vào Command Register để reset toàn bộ chip về mặc định
        port_cr.write(0x10);

        // Chờ cho đến khi bit RST quay về mức 0 (QEMU phản hồi rất nhanh, nhưng thật vẫn cần vòng lặp)
        while (port_cr.read() & 0x10) != 0 {
            core::hint::spin_loop();
        }
        println!("RTL8139: Reset complete.");

        // -------------------------------------------------------------------------
        // Bước 2: Thiết lập Bộ đệm Nhận (RX Buffer)
        // -------------------------------------------------------------------------
        // LẤY ĐỊA CHỈ VẬT LÝ: Bản chất mảng `self.rx_buffer` nằm trên RAM.
        // LƯU Ý: Nếu OS của bạn đã bật Paging (Phân trang), bạn PHẢI chuyển đổi
        // địa chỉ ảo này sang địa chỉ vật lý tương ứng.
        // Dưới đây giả định Identity Mapping (Địa chỉ ảo == Địa chỉ vật lý) cho đơn giản:
        let rx_buffer_phys_addr = self.rx_buffer.as_ptr() as u32;

        // Ghi địa chỉ vùng nhớ nhận dữ liệu vào thanh ghi RBSTART
        port_rbstart.write(rx_buffer_phys_addr);

        // -------------------------------------------------------------------------
        // Bước 3: Cấu hình bộ nhận (Receive Configuration)
        // -------------------------------------------------------------------------
        // Cấu hình các bộ lọc gói tin (Accept Options):
        // Bit 0 (AAP): Chấp nhận mọi gói tin (Promiscuous mode)
        // Bit 1 (APM): Chấp nhận gói tin trùng Physical Mac của card
        // Bit 2 (AM): Chấp nhận gói tin Multicast
        // Bit 3 (AB): Chấp nhận gói tin Broadcast
        // Bit 7 (WRAP): Khi bộ đệm đầy, tự động quay vòng ghi đè lên đầu (Rất quan trọng!)
        let rcr_config: u32 = (1 << 7) | (1 << 3) | (1 << 2) | (1 << 1) | (1 << 0);
        port_rcr.write(rcr_config);

        // -------------------------------------------------------------------------
        // Bước 4: Xóa trạng thái ngắt cũ và Bật Ngắt (Interrupts)
        // -------------------------------------------------------------------------
        // Ghi 0xFFFF vào ISR để clear mọi ngắt rác còn sót lại trước đó
        port_isr.write(0xFFFF);

        // Cấu hình IMR để chọn các sự kiện sẽ gửi ngắt (IRQ) lên CPU:
        // Bit 0 (ROK): Receive OK (Có gói tin đi vào)
        // Bit 2 (TOK): Transmit OK (Gửi gói tin thành công)
        // Ghi 0x0005 để bật cả 2 ngắt này
        port_imr.write(0x0005);

        // -------------------------------------------------------------------------
        // Bước 5: Kích hoạt chức năng Truyền/Nhận (Enable RE & TE)
        // -------------------------------------------------------------------------
        // Bit 2 (TE): Transmit Enable (Bật gửi)
        // Bit 3 (RE): Receive Enable (Bật nhận)
        port_cr.write(0x0C);

        println!("RTL8139: Driver successfully initialized and listening for packets.");
    }
}

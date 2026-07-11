// 1. Khai báo file print.rs là một submodule con
pub mod print;

// 2. Kéo hàm ẩn `_print` ra lớp vỏ ngoài để khớp với đường dẫn của macro
// Đường dẫn này bắt buộc phải khớp với `$crate::vga_buffer::_print`
pub use self::print::_print;

// 3. Re-export (Công khai) các cấu trúc dữ liệu quan trọng ra ngoài vga_buffer
// Việc này cho phép bạn gọi dạng: `something_os::vga_buffer::Color`
pub use self::print::{Color, Writer, WRITER};

# Countdown — thiết kế triển khai

Ngày: 2026-10-16 · Nguồn: `Countdown — Đồng hồ đếm ngược không khung.md` + prototype khung 00 trên canvas.

## Mục tiêu

App đếm ngược không khung, chạy nền từ lúc đăng nhập, gọi lên bằng Ctrl+Alt+Space, tiêu thụ tối thiểu RAM/CPU. Luồng dùng: phím tắt → gõ số → ↑↓ chọn âm (nghe thử) → Enter.

## Quyết định đã chốt

| Vấn đề | Chọn |
| --- | --- |
| Framework | Rust native: x11rb + cairo-xcb + pango (font variable), âm tổng hợp ghi ra PulseAudio |
| Hiển thị | X11 qua XWayland (Wayland không cho client tự đặt vị trí/always-on-top) |
| Màu chữ | Đọc file hình nền GNOME (gsettings qua dconf mount chỉ đọc), đo độ sáng vùng giữa mép trên, kiểm tra mỗi 10 s; phím T đảo tay |
| Kéo | Không; cố định giữa mép trên màn hình chính, cách 28 px |
| Cài đặt | Phím tắt + lưu `/data/config.json` (volume Docker) |
| Phím tắt toàn cục | Ctrl+Alt+Space = custom keybinding GNOME, ghi `show` vào FIFO `$XDG_RUNTIME_DIR/countdown/ctl` |
| Cài đặt / tự khởi động | `docker run --rm pythonking11/countdown install \| sh` (từ 0.2.0): script in từ image cài `countdown-run`, `countdown-toggle` và XDG autostart; không dùng compose hay systemd |
| Ẩn | Tự ẩn 8 s sau khi về 0; Esc/phím tắt khi đang nhập → ẩn; Esc khi đang chạy → hủy và ẩn |

## Kiến trúc

Một binary, hai luồng: luồng chính (X11 + FIFO + máy trạng thái + vẽ, `poll()` với timeout = thời điểm khung/tick kế tiếp) và luồng âm thanh (nhận lệnh qua channel, tổng hợp, ghi libpulse-simple; đóng stream khi im).

| Module | Trách nhiệm |
| --- | --- |
| `timer.rs` | Máy trạng thái thuần: Hidden/Setup/Running/Paused/Done, bộ đệm nhập số kiểu lò vi sóng (≤ 99:59, không nhận 0 đầu), đồng hồ đơn điệu |
| `anim.rs` | Hàm thuần theo thời gian: 5 mảnh vỡ (1,3 s), tan chảy (1,1 s), rơi + nảy (0,62 s, trễ 80 ms), độ căng t = (10−r)/9, rung 3 cấp, bật lại khi về 0 |
| `render.rs` | cairo/pango: phút viền rỗng, giây chữ đặc, dấu hai chấm, con trỏ, dòng chữ nhỏ, bóng chữ; trả về các hình chữ nhật nhận chuột |
| `window.rs` | Cửa sổ ARGB 32-bit, không viền (`_MOTIF_WM_HINTS`), ABOVE + SKIP_TASKBAR/PAGER, vị trí theo RandR primary, XShape input region, map/unmap, lấy focus qua `_NET_ACTIVE_WINDOW` (source=2) + `SetInputFocus` |
| `audio/synth.rs` | Port graph Web Audio của prototype: nhiễu trắng/hồng/nâu, biquad (RBJ), LFO, ramp gain, tiếng củi nổ; chuông báo thức hai quả (búa gõ 24 lần/s, 1 hồi ~0,9 s) khi hết giờ |
| `audio/mod.rs` | Luồng âm thanh + output Pulse |
| `wallpaper.rs` | Lấy URI hình nền theo `color-scheme`, giải mã, tính độ sáng (Rec. 709) vùng giữa mép trên theo scale "zoom"; cache theo đường dẫn + mtime |
| `config.rs` | Đọc/ghi JSON: sound, muted, volume, transition |
| `ctl.rs` | Tạo và đọc FIFO lệnh (`show`, `hide`, `quit`, `type <số>`, `enter`) |

## Hành vi

- Phím: số/Backspace nhập; Enter/click start-pause-resume; Esc như bảng trên; ↑↓/lăn chuột đổi âm (nghe thử ở âm lượng thấp khi đang nhập); M tắt tiếng; + / − âm lượng (bước 0,05); S đổi vỡ vụn/tan chảy; T đảo màu; Q ẩn.
- Âm: lên dần 2,5 s khi bắt đầu/tiếp tục; tạm dừng nhỏ dần 0,6 s; 10 giây cuối gain = vol·(0,15 + 0,85·r/10); về 0 nền tắt 1,2 s, chuông reo 1 hồi (bỏ qua khi tắt tiếng; dừng ngay khi ẩn).
- Dòng chữ nhỏ JetBrains Mono 11 px, giãn 0,16em, hiện ~2 s; lúc gọi lên hiện "ÂM THANH — <tên> · ↑ ↓ ĐỂ ĐỔI".

## Tài nguyên

Chữ được raster một lần thành sprite trên X server (kèm bóng làm mờ thật); mỗi khung chỉ ghép sprite bằng XRender. Đo thực tế: ẩn ~4 MB / 0% CPU; đang đếm ~1,5% CPU app + ~3% Xwayland.


Vẽ 60 fps chỉ khi có animation, còn lại chỉ vẽ lại khi trạng thái đổi hoặc nhịp nháy 0,5 s. Ẩn: không vẽ, không âm thanh. Mục tiêu: RAM < 30 MB, CPU ≈ 0 khi ẩn.

## Docker

Dockerfile nhiều stage (`builder` → `test` → runtime `debian:bookworm-slim` với libcairo2, libpango, libpulse0, libglib2.0-bin, dconf-gsettings-backend, font Archivo + JetBrains Mono). Compose mount: `/tmp/.X11-unix`, XAUTHORITY, socket Pulse, dconf + thư mục hình nền (chỉ đọc), `$XDG_RUNTIME_DIR/countdown`, volume `countdown-data`. Chạy uid 1000.

## Rủi ro (spike trước)

Cửa sổ trong suốt/vị trí/always-on-top qua XWayland; XShape; lấy focus khi gọi từ phím tắt; Pulse từ container; FIFO host → container. Tan chảy là bản xấp xỉ (không có filter goo).

## Kiểm thử

Unit test: timer, bộ đệm nhập, tham số độ căng, synth (hữu hạn, |x| ≤ 1), độ sáng ảnh, config. Thủ công: chạy trên màn hình thật, đo `docker stats`.

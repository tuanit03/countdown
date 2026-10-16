# Countdown

Đồng hồ đếm ngược không khung: chỉ có con số, lơ lửng giữa mép trên màn hình, kèm âm thanh nền giúp tập trung. Thiết kế: `Countdown — Đồng hồ đếm ngược không khung.md`.

## Cài đặt (một lần)

```sh
./install.sh
```

Script build image Docker, cài `countdown.service` (systemd user, tự chạy mỗi lần đăng nhập) và gán phím **Ctrl+Alt+Space**. Gỡ: `./install.sh --uninstall`.

Yêu cầu: GNOME (Wayland hoặc Xorg) có XWayland, Docker, PipeWire/PulseAudio.

Image dựng sẵn: `docker pull pythonking11/countdown:0.1.0`, rồi `docker tag pythonking11/countdown:0.1.0 countdown:latest`; sau đó các bước dưới không cần build.

Container chạy với quyền tối thiểu: không mạng, root fs chỉ đọc, bỏ mọi capability, uid 1000. Nó vẫn cần socket X11 và PulseAudio của phiên đăng nhập; quyền truy cập hai socket này tương đương một app desktop bình thường.

## Dùng

**Ctrl+Alt+Space → gõ số → ↑ ↓ chọn âm → Enter.** Gõ kiểu lò vi sóng: `2 5 0 0` = 25:00, `4 2 5` = 04:25.

| Phím | Tác dụng |
| --- | --- |
| Số (hàng trên hoặc bàn phím số, bật/tắt NumLock đều được), Backspace | Nhập thời gian |
| Enter / click | Bắt đầu · tạm dừng · tiếp tục |
| Esc, Q | Hủy và ẩn |
| ↑ ↓ (phím mũi tên) / lăn chuột | Đổi âm (mưa, sóng biển, tiếng ồn nâu, gió rừng, lửa trại), nghe thử khi đang nhập |
| M | Tắt/bật tiếng |
| + / − | Âm lượng |
| S | Chuyển cảnh: vỡ vụn / tan chảy |
| T | Đảo màu chữ (khi tự nhận hình nền sai) |

Hết giờ: chuông reo, tự ẩn sau 8 giây (click hoặc Esc để ẩn ngay). Âm, âm lượng, kiểu chuyển cảnh được nhớ cho lần sau.

Có thể điều khiển bằng script: `countdown-toggle "type 2500" enter` đặt 25:00 và chạy luôn.

## Phát triển

Máy không cần cài Rust; mọi thứ build trong Docker:

```sh
docker build --target builder -t countdown-dev .          # image dev (cache dependencies)
docker run --rm -v "$PWD":/w -v countdown-target:/w/target -w /w countdown-dev cargo test --release
docker build --target test .                               # chạy test như CI
docker compose build && systemctl --user restart countdown  # cập nhật bản đang chạy
```

Chạy thử không qua service: `docker compose run --rm countdown --show`.

## License

Mã nguồn: MIT (`LICENSE`). Font Archivo và JetBrains Mono: SIL OFL 1.1, file license đi kèm trong image tại `/usr/share/fonts/truetype/countdown/`.

# Countdown

Đồng hồ đếm ngược không khung: chỉ có con số, lơ lửng giữa mép trên màn hình, kèm âm thanh nền giúp tập trung. Thiết kế: `Countdown — Đồng hồ đếm ngược không khung.md`.

## Cài đặt

Trên bất kỳ máy Linux nào có Docker, chạy trong terminal của phiên desktop (không cần clone repo, không cần root):

```sh
docker run --rm pythonking11/countdown install | sh
```

Gỡ cài đặt:

```sh
docker run --rm pythonking11/countdown uninstall | sh
```

Lệnh `install` chỉ in ra một script (xem trước: `docker run --rm pythonking11/countdown install > install.sh`). Script cài vào thư mục home của bạn:

- `~/.local/bin/countdown-run`: chạy container với quyền truy cập màn hình, âm thanh, hình nền của phiên hiện tại.
- `~/.local/bin/countdown-toggle`: gọi đồng hồ lên; trên GNOME được gán sẵn vào **Ctrl+Alt+Space**. Desktop khác: tự gán một phím tắt tới lệnh này.
- `~/.config/autostart/countdown.desktop`: chạy nền mỗi lần đăng nhập.

Container không thể tự truy cập màn hình hay loa của máy (Docker cách ly theo thiết kế), vì vậy cần script này thay vì `docker run` trơn.

| Môi trường | Hỗ trợ |
| --- | --- |
| GNOME (Wayland hoặc Xorg) | Đầy đủ, phím tắt tự gán, màu chữ theo hình nền |
| KDE, XFCE, Cinnamon… có compositor | Chạy được; tự gán phím tắt; đổi màu chữ bằng phím T |
| Wayland không có XWayland | Không hiển thị được |
| Không có compositor | Chạy được nhưng nền không trong suốt |
| Không có PipeWire/PulseAudio | Chạy được, không có tiếng |

Container chạy với quyền tối thiểu: không mạng, root fs chỉ đọc, bỏ mọi capability, chạy bằng uid của bạn. Nó vẫn cần socket X11 và PulseAudio của phiên đăng nhập; quyền truy cập hai socket này tương đương một app desktop bình thường.

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
./install.sh                                               # build image local rồi cài/cập nhật bản đang chạy
./install.sh --uninstall
```

Phát hành: `docker build -t pythonking11/countdown:X.Y.Z -t pythonking11/countdown:latest --build-arg IMAGE_REF=pythonking11/countdown:X.Y.Z .` rồi push cả hai tag.

## License

Mã nguồn: MIT (`LICENSE`). Font Archivo và JetBrains Mono: SIL OFL 1.1, file license đi kèm trong image tại `/usr/share/fonts/truetype/countdown/`.

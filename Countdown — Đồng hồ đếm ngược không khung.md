# Countdown — Đồng hồ đếm ngược không khung

Oct 16, 2026 · @heisenberg

## Tổng quan

Countdown là một app desktop đếm ngược chỉ gồm con số: không khung cửa sổ, không nút bấm, lơ lửng ở giữa mép trên màn hình và chạy kèm âm thanh nền giúp tập trung. App được khởi chạy bằng `docker compose up` hoặc `docker run`.

Toàn bộ thiết kế và một bản chạy thử tương tác nằm trên canvas: [Countdown — Typography phá cách](https://claude.ai/artifact/46hUhzLAFeijWV4A2eoj2k). Khung 00 trên canvas là bản chạy thật (bấm Play), các khung 01 đến 07 là ảnh tĩnh của từng trạng thái.

## Ý tưởng thiết kế

Con số là toàn bộ thiết kế: không nền, không viền, không thanh tiêu đề, không nút. Hình dạng "cửa sổ" chính là các chữ số, nên nó thay đổi mỗi giây.

- **Typography phá cách:** phút viết dạng viền rỗng, giây là chữ đặc. Mỗi giây số cũ vỡ vụn (hoặc tan chảy) và số mới rơi xuống.
- **Không chiếm diện tích:** chữ cao 120px, đặt chính giữa mép trên màn hình, cách mép 28px. Chỉ lúc số vỡ, các mảnh mới rơi lấn xuống vùng làm việc trong chốc lát.
- **Không nút:** mọi thao tác qua click, bàn phím và con lăn chuột trên chính con số.
- **Tự đổi màu theo hình nền:** chữ trắng ngà trên nền tối, chữ đen trên nền sáng, để luôn đọc được.

## Luồng thao tác

&#91;embedded content: luồng thao tác · 4 trạng thái, chỉ dùng click\]

Khi mở lên app hiện 00:00 và chờ gõ số kiểu lò vi sóng (gõ 4, 2, 5 thành 04:25, tối đa 99:59). Click lần 1 bắt đầu, lần 2 tạm dừng, lần 3 chạy tiếp; Enter tương đương một click.

## Các khung trên canvas

Canvas có 8 khung, mỗi khung là một màn hình desktop 1440×900 với hình nền và hai cửa sổ app mô phỏng.

| Khung | Trạng thái | Nội dung chính |
| --- | --- | --- |
| 00 | Bản chạy thử | Chạy thật: gõ số, click, đổi âm thanh, chỉnh Tweaks |
| 01 | Nhập thời gian | 04:25 đang gõ, chữ số chưa nhập ở dạng viền rỗng, con trỏ cam |
| 02 | Đang chạy | Mảnh vỡ của 27 rơi xuống desktop, 26 rơi vào chỗ |
| 03 | Tạm dừng | Con số mờ còn khoảng 40%, dấu hai chấm ngừng nhấp nháy |
| 04 | Hình nền sáng | Chữ tự chuyển sang màu đen |
| 05 | 10 giây cuối | Chữ đậm, hẹp, nghiêng, chuyển cam đỏ, có bóng rung |
| 06 | Về 0 | 00:00 phát sáng màu cam, click để nhập lại |
| 07 | Đổi âm thanh | Dòng chữ nhỏ dưới con số báo âm thanh đang phát |

## Hiệu ứng chuyển giây và 10 giây cuối

Mỗi giây có một chuyển cảnh; 10 giây cuối chữ "căng" dần lên theo từng giây.

- **Vỡ vụn (mặc định):** số cũ tách thành 5 mảnh, bật nhẹ ra rồi rơi theo trọng lực trong 1,3 giây, xoay và mờ dần.
- **Tan chảy:** số cũ giãn dài xuống, nhòe và dính nét như chất lỏng rồi biến mất trong 1,1 giây.
- **Số mới:** rơi từ trên xuống, nảy nhẹ khi vào chỗ (0,6 giây).
- **10 giây cuối:** độ đậm tăng từ 560 lên 900, độ rộng hẹp từ 74 xuống 62, nghiêng tới 12°, màu chuyển dần sang cam đỏ #FF4D1F, rung theo 3 cấp tăng dần.
- **Về 0:** chữ bật thẳng lại, phóng to rồi thu về, toàn bộ 00:00 màu cam.

## Âm thanh tập trung

Trong lúc đếm có âm thanh nền nhẹ, chọn từ 5 âm có sẵn, tất cả được tạo bằng Web Audio nên không cần file và không vướng bản quyền.

| Âm thanh | Cảm giác |
| --- | --- |
| Mưa (mặc định) | Mưa rào đều, dao động nhẹ |
| Sóng biển | Tiếng trầm dâng lên rút xuống, khoảng 12 giây một nhịp |
| Tiếng ồn nâu | Tiếng rì trầm, đều tuyệt đối |
| Gió rừng | Gió thổi, cao độ và cường độ đổi rất chậm |
| Lửa trại | Tiếng lửa trầm kèm tiếng củi nổ lách tách |

- **Chọn:** phím ↑ ↓ hoặc lăn chuột trên con số; dòng chữ "ÂM THANH — …" hiện dưới số khoảng 2 giây. Phím M để tắt/bật.
- **Bắt đầu / tiếp tục:** âm lên dần trong 2,5 giây. **Tạm dừng:** nhỏ dần rồi im.
- **10 giây cuối:** âm nhỏ dần theo từng giây. **Về 0:** nền im, vang một tiếng chuông 3 nốt (bỏ qua khi đang tắt tiếng).
- **Khởi động:** dòng gợi ý "ÂM THANH — MƯA · ↑ ↓ ĐỂ ĐỔI" hiện vài giây.

## Hệ thống hình ảnh

Hai font, hai màu chữ theo hình nền và một màu nhấn chỉ dùng khi căng thẳng hoặc hết giờ.

| Thành phần | Giá trị |
| --- | --- |
| Font chữ số | Archivo (variable: độ đậm 100–900, độ rộng 62–125) |
| Font chữ nhỏ | JetBrains Mono, 11–12px, giãn chữ 0,14–0,16em |
| Chữ trên nền tối | #F1EDE4 (trắng ngà) |
| Chữ trên nền sáng | #0C0C0B (đen than) |
| Màu nhấn | #FF4D1F (cam đỏ) |
| Cỡ chữ số | 120px |
| Vị trí | Giữa mép trên màn hình, cách mép 28px |
| Trạng thái thường | Độ đậm 560, độ rộng 74 |
| Tạm dừng | Độ mờ 38% |

## Ghi chú kỹ thuật

Rủi ro lớn nhất là chạy một cửa sổ trong suốt, có tiếng, từ trong container; nên thử phần này trước khi làm sâu vào hiệu ứng.

- **Hiển thị từ Docker:** container không tự hiện cửa sổ. Trên Linux cần chia sẻ X11/Wayland (biến `DISPLAY`, thư mục `/tmp/.X11-unix`); macOS cần XQuartz; Windows dùng WSLg hoặc một X server.
- **Cửa sổ trong suốt, không viền:** Electron (`transparent: true`, `frame: false`) hoặc Tauri đều làm được, nhưng cần trình quản lý cửa sổ có hỗ trợ trong suốt (compositor).
- **Click xuyên qua:** vùng trống giữa các chữ số nên để chuột bấm xuyên xuống app bên dưới (Electron: `setIgnoreMouseEvents`); chỉ nét chữ nhận click, kéo và lăn chuột.
- **Âm thanh từ Docker:** cần mount socket PulseAudio hoặc PipeWire của máy vào container, nếu không app chạy nhưng không có tiếng.
- **Luôn trên cùng và nhận phím:** cửa sổ cần chế độ always-on-top và nhận focus khi khởi động để gõ số ngay.

## Việc còn mở và bước tiếp theo

- [ ] Quyết định cách đặt lại giữa chừng khi nhập nhầm (đề xuất: phím Esc về trạng thái nhập số)
- [ ] Chọn framework: Electron hay Tauri
- [ ] Thử nghiệm sớm: cửa sổ trong suốt + click xuyên qua + âm thanh chạy từ Docker trên máy thật
- [ ] Cách đọc độ sáng hình nền phía sau con số để tự đổi màu chữ
- [ ] Viết `Dockerfile` và `docker-compose.yml` với cấu hình hiển thị và âm thanh
- [ ] Cân nhắc nhớ âm thanh và âm lượng đã chọn cho lần mở sau

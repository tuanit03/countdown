//! Frameless, transparent, always-on-top X11 window with a shaped input region.

use std::error::Error;
use x11rb::connection::Connection;
use x11rb::properties::{WmHints, WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

pub type Res<T> = Result<T, Box<dyn Error>>;

pub const WIDTH: u16 = 640;
pub const HEIGHT: u16 = 600;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_WM_NAME,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WM_STATE_STICKY,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_ACTIVE_WINDOW,
        _NET_WM_USER_TIME,
        _MOTIF_WM_HINTS,
        UTF8_STRING,
        COUNTDOWN_TIME,
    }
}

/// Layout matches libxcb's `xcb_visualtype_t`, which cairo-xcb reads.
#[repr(C)]
struct XcbVisualtype {
    visual_id: u32,
    class: u8,
    bits_per_rgb_value: u8,
    colormap_entries: u16,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    pad0: [u8; 4],
}

pub struct Window {
    pub conn: XCBConnection,
    pub win: u32,
    root: u32,
    atoms: Atoms,
    pixmap: u32,
    gc: u32,
    pub surface: cairo::XCBSurface,
    _visual: Box<XcbVisualtype>,
    pub mapped: bool,
    /// (x, y, w, h) of the monitor the window sits on.
    pub monitor: (i16, i16, u16, u16),
    keymap: (u8, u8, Vec<u32>),
    /// Modifier bit NumLock is bound to (usually Mod2).
    numlock: u16,
    pub last_time: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Key { sym: u32, ctrl: bool },
    Click,
    Wheel(i8),
    /// Server timestamp obtained for focusing after map.
    Time(u32),
    Expose,
}

impl Window {
    pub fn new() -> Res<Window> {
        let (conn, screen_num) = XCBConnection::connect(None)?;
        let screen = conn.setup().roots[screen_num].clone();
        let root = screen.root;
        let atoms = Atoms::new(&conn)?.reply()?;

        let vt = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .copied()
            .ok_or("no 32-bit ARGB visual (is a compositor running?)")?;

        let cmap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, cmap, root, vt.visual_id)?;
        let win = conn.generate_id()?;
        let events = EventMask::EXPOSURE | EventMask::KEY_PRESS | EventMask::BUTTON_PRESS | EventMask::STRUCTURE_NOTIFY | EventMask::PROPERTY_CHANGE;
        conn.create_window(
            32,
            win,
            root,
            0,
            0,
            WIDTH,
            HEIGHT,
            0,
            WindowClass::INPUT_OUTPUT,
            vt.visual_id,
            &CreateWindowAux::new().background_pixel(0).border_pixel(0).colormap(cmap).event_mask(events),
        )?;

        conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_NAME, AtomEnum::STRING, b"Countdown")?;
        conn.change_property8(PropMode::REPLACE, win, atoms._NET_WM_NAME, atoms.UTF8_STRING, b"Countdown")?;
        conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_CLASS, AtomEnum::STRING, b"countdown\0Countdown\0")?;
        // No decorations.
        conn.change_property32(PropMode::REPLACE, win, atoms._MOTIF_WM_HINTS, atoms._MOTIF_WM_HINTS, &[2, 0, 0, 0, 0])?;
        let wtype = match std::env::var("COUNTDOWN_WINDOW_TYPE").as_deref() {
            Ok("normal") => atoms._NET_WM_WINDOW_TYPE_NORMAL,
            Ok("dock") => atoms._NET_WM_WINDOW_TYPE_DOCK,
            Ok("notification") => atoms._NET_WM_WINDOW_TYPE_NOTIFICATION,
            _ => atoms._NET_WM_WINDOW_TYPE_UTILITY,
        };
        conn.change_property32(PropMode::REPLACE, win, atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM, &[wtype])?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms._NET_WM_STATE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_STATE_ABOVE, atoms._NET_WM_STATE_SKIP_TASKBAR, atoms._NET_WM_STATE_SKIP_PAGER, atoms._NET_WM_STATE_STICKY],
        )?;
        let hints = WmHints { input: Some(true), ..WmHints::default() };
        hints.set(&conn, win)?;

        let pixmap = conn.generate_id()?;
        conn.create_pixmap(32, pixmap, win, WIDTH, HEIGHT)?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, pixmap, &CreateGCAux::new().graphics_exposures(0))?;

        let mut visual = Box::new(XcbVisualtype {
            visual_id: vt.visual_id,
            class: u8::from(vt.class),
            bits_per_rgb_value: vt.bits_per_rgb_value,
            colormap_entries: vt.colormap_entries,
            red_mask: vt.red_mask,
            green_mask: vt.green_mask,
            blue_mask: vt.blue_mask,
            pad0: [0; 4],
        });
        let surface = unsafe {
            let cconn = cairo::XCBConnection::from_raw_none(conn.get_raw_xcb_connection() as _);
            let cvis = cairo::XCBVisualType::from_raw_none(&mut *visual as *mut XcbVisualtype as _);
            cairo::XCBSurface::create(&cconn, &cairo::XCBDrawable(pixmap), &cvis, WIDTH as i32, HEIGHT as i32)?
        };

        let (keymap, numlock) = load_keymap(&conn)?;
        let w = Window {
            conn,
            win,
            root,
            atoms,
            pixmap,
            gc,
            surface,
            _visual: visual,
            mapped: false,
            monitor: (0, 0, 1920, 1080),
            keymap,
            numlock,
            last_time: 0,
        };
        w.set_input_rects(&[])?;
        w.conn.flush()?;
        Ok(w)
    }

    pub fn fd(&self) -> i32 {
        use std::os::unix::io::AsRawFd;
        self.conn.as_raw_fd()
    }

    /// Primary monitor geometry via RandR, falling back to the whole root window.
    fn primary_monitor(&self) -> (i16, i16, u16, u16) {
        let screen = &self.conn.setup().roots[0];
        let fallback = (0, 0, screen.width_in_pixels, screen.height_in_pixels);
        let Ok(Ok(r)) = self.conn.randr_get_monitors(self.root, true).map(|c| c.reply()) else {
            return fallback;
        };
        r.monitors.iter().find(|m| m.primary).or(r.monitors.first()).map_or(fallback, |m| (m.x, m.y, m.width, m.height))
    }

    pub fn show(&mut self) -> Res<()> {
        self.monitor = self.primary_monitor();
        let (mx, my, mw, _) = self.monitor;
        let x = mx as i32 + (mw as i32 - WIDTH as i32) / 2;
        let y = my as i32;
        let mut sh = WmSizeHints::new();
        sh.position = Some((WmSizeHintsSpecification::UserSpecified, x, y));
        sh.size = Some((WmSizeHintsSpecification::UserSpecified, WIDTH as i32, HEIGHT as i32));
        sh.min_size = Some((WIDTH as i32, HEIGHT as i32));
        sh.max_size = Some((WIDTH as i32, HEIGHT as i32));
        sh.set_normal_hints(&self.conn, self.win)?;
        self.conn.configure_window(self.win, &ConfigureWindowAux::new().x(x).y(y))?;
        if !self.mapped {
            self.conn.map_window(self.win)?;
            self.mapped = true;
        }
        self.conn.configure_window(self.win, &ConfigureWindowAux::new().x(x).y(y).stack_mode(StackMode::ABOVE))?;
        self.request_time()?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn hide(&mut self) -> Res<()> {
        if self.mapped {
            self.conn.unmap_window(self.win)?;
            self.mapped = false;
            self.conn.flush()?;
        }
        Ok(())
    }

    /// Ask for a server timestamp (arrives as `Input::Time`) so focus requests aren't treated as stale.
    pub fn request_time(&self) -> Res<()> {
        self.conn.change_property8(PropMode::APPEND, self.win, self.atoms.COUNTDOWN_TIME, AtomEnum::STRING, &[])?;
        Ok(())
    }

    pub fn focus(&mut self, time: u32) -> Res<()> {
        if !self.mapped {
            return Ok(());
        }
        // source = 2 (pager): window managers honour these without focus-stealing prevention.
        let ev = ClientMessageEvent::new(32, self.win, self.atoms._NET_ACTIVE_WINDOW, [2, time, 0, 0, 0]);
        self.conn.send_event(false, self.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev)?;
        self.conn.change_property32(PropMode::REPLACE, self.win, self.atoms._NET_WM_USER_TIME, AtomEnum::CARDINAL, &[time])?;
        let _ = self.conn.set_input_focus(InputFocus::PARENT, self.win, time);
        self.conn.flush()?;
        Ok(())
    }

    /// Only these rectangles take pointer input; everything else clicks through.
    pub fn set_input_rects(&self, rects: &[(i16, i16, u16, u16)]) -> Res<()> {
        let r: Vec<Rectangle> = rects.iter().map(|&(x, y, width, height)| Rectangle { x, y, width, height }).collect();
        self.conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, self.win, 0, 0, &r)?;
        Ok(())
    }

    /// Copy the back buffer to the window.
    pub fn present(&self) -> Res<()> {
        self.surface.flush();
        self.conn.copy_area(self.pixmap, self.win, self.gc, 0, 0, 0, 0, WIDTH, HEIGHT)?;
        self.conn.flush()?;
        Ok(())
    }

    fn keysym(&self, code: u8, shift: bool, numlock: bool) -> u32 {
        let (min, per, syms) = &self.keymap;
        let base = (code.saturating_sub(*min)) as usize * *per as usize;
        let end = (base + *per as usize).min(syms.len());
        pick_keysym(syms.get(base..end).unwrap_or(&[]), shift, numlock)
    }

    /// Translate pending X events. Returns Err when the display connection is gone.
    pub fn poll_events(&mut self, out: &mut Vec<Input>) -> Res<()> {
        while let Some(ev) = self.conn.poll_for_event()? {
            use x11rb::protocol::Event as E;
            match ev {
                E::KeyPress(k) => {
                    self.last_time = k.time;
                    let state = u16::from(k.state);
                    let shift = state & u16::from(KeyButMask::SHIFT) != 0;
                    let ctrl = state & u16::from(KeyButMask::CONTROL) != 0;
                    let numlock = state & self.numlock != 0;
                    out.push(Input::Key { sym: self.keysym(k.detail, shift, numlock), ctrl });
                }
                E::ButtonPress(b) => {
                    self.last_time = b.time;
                    match b.detail {
                        1 => out.push(Input::Click),
                        4 => out.push(Input::Wheel(-1)),
                        5 => out.push(Input::Wheel(1)),
                        _ => {}
                    }
                }
                E::PropertyNotify(p) if p.atom == self.atoms.COUNTDOWN_TIME => {
                    self.last_time = p.time;
                    out.push(Input::Time(p.time));
                }
                E::Expose(e) if e.count == 0 => out.push(Input::Expose),
                E::MappingNotify(_) => {
                    (self.keymap, self.numlock) = load_keymap(&self.conn)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn flush(&self) -> Res<()> {
        self.conn.flush()?;
        Ok(())
    }
}

const NUM_LOCK: u32 = 0xff7f;

fn is_keypad(sym: u32) -> bool {
    (0xff80..=0xffbd).contains(&sym)
}

/// Core-protocol keysym selection: NumLock switches keypad keys to their second (digit) level,
/// where Shift then flips back; elsewhere Shift picks the second level.
pub fn pick_keysym(syms: &[u32], shift: bool, numlock: bool) -> u32 {
    let at = |i: usize| syms.get(i).copied().unwrap_or(0);
    let (lower, upper) = (at(0), if at(1) != 0 { at(1) } else { at(0) });
    if numlock && is_keypad(upper) {
        return if shift { lower } else { upper };
    }
    if shift { upper } else { lower }
}

type Keymap = (u8, u8, Vec<u32>);

/// Keyboard mapping plus the modifier mask carrying NumLock.
fn load_keymap(conn: &XCBConnection) -> Res<(Keymap, u16)> {
    let min = conn.setup().min_keycode;
    let max = conn.setup().max_keycode;
    let r = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    let per = r.keysyms_per_keycode as usize;
    let mods = conn.get_modifier_mapping()?.reply()?;
    let per_mod = mods.keycodes_per_modifier() as usize;
    let mut numlock = 0u16;
    for (i, codes) in mods.keycodes.chunks(per_mod.max(1)).enumerate().take(8) {
        let has_numlock = codes.iter().any(|&c| {
            c >= min && {
                let base = (c - min) as usize * per;
                r.keysyms.get(base..base + per).is_some_and(|s| s.contains(&NUM_LOCK))
            }
        });
        if has_numlock {
            numlock |= 1 << i;
        }
    }
    // Fall back to Mod2, where NumLock lives on practically every layout.
    if numlock == 0 {
        numlock = u16::from(KeyButMask::MOD2);
    }
    Ok(((min, r.keysyms_per_keycode, r.keysyms), numlock))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KP_UP: u32 = 0xff97;
    const KP_8: u32 = 0xffb8;

    #[test]
    fn numlock_selects_keypad_digits() {
        let kp8 = [KP_UP, KP_8, KP_UP, KP_8];
        assert_eq!(pick_keysym(&kp8, false, true), KP_8);
        assert_eq!(pick_keysym(&kp8, true, true), KP_UP);
        assert_eq!(pick_keysym(&kp8, false, false), KP_UP);
        assert_eq!(pick_keysym(&kp8, true, false), KP_8);
    }

    #[test]
    fn numlock_leaves_other_keys_alone() {
        let a = [0x61, 0x41];
        assert_eq!(pick_keysym(&a, false, true), 0x61);
        assert_eq!(pick_keysym(&a, true, true), 0x41);
        assert_eq!(pick_keysym(&[0xff0d], true, false), 0xff0d);
    }
}

//! Системный буфер обмена для картинок (arboard) и клавиша «Вставить».
//!
//! egui-winit превращает Ctrl+V в событие вставки, только если в буфере есть текст: картинка без
//! текста до программы не доходит. Поэтому, пока зажат Ctrl, клавиша V опрашивается у системы
//! напрямую (X11 — `QueryKeymap`, Windows — `GetAsyncKeyState`). Где опросить нельзя (родной
//! Wayland), остаётся событие egui.

/// Картинка буфера: ширина, высота, RGBA по 8 бит без предумножения.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Clip {
    board: Option<arboard::Clipboard>,
    key: PasteKey,
}

impl Clip {
    pub fn new() -> Self {
        Self { board: None, key: PasteKey::new() }
    }

    fn board(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.board.is_none() {
            self.board = arboard::Clipboard::new().ok();
        }
        self.board.as_mut()
    }

    /// Положить картинку в буфер системы; `false` — буфера нет.
    pub fn set_image(&mut self, img: &Image) -> bool {
        let data = arboard::ImageData { width: img.width as usize, height: img.height as usize, bytes: std::borrow::Cow::Borrowed(&img.rgba) };
        self.board().is_some_and(|b| b.set_image(data).is_ok())
    }

    pub fn get_image(&mut self) -> Option<Image> {
        let img = self.board()?.get_image().ok()?;
        Some(Image { width: img.width as u32, height: img.height as u32, rgba: img.bytes.into_owned() })
    }

    /// Ctrl+V нажата в этом кадре. `egui_paste` — пришло ли событие вставки от egui (им
    /// пользуемся, только если клавишу опросить нельзя).
    pub fn paste_pressed(&mut self, ctrl: bool, egui_paste: bool) -> bool {
        match self.key.down(ctrl) {
            Some(down) => {
                let edge = down && !self.key.was;
                self.key.was = down;
                edge
            }
            None => egui_paste,
        }
    }
}

struct PasteKey {
    was: bool,
    #[cfg(xdg)]
    x11: Option<(x11rb::rust_connection::RustConnection, u8)>,
    #[cfg(xdg)]
    tried: bool,
}

impl PasteKey {
    fn new() -> Self {
        Self {
            was: false,
            #[cfg(xdg)]
            x11: None,
            #[cfg(xdg)]
            tried: false,
        }
    }

    /// Зажата ли V (при зажатом Ctrl); `None` — опросить нельзя.
    #[cfg(xdg)]
    fn down(&mut self, ctrl: bool) -> Option<bool> {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::ConnectionExt as _;
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return None;
        }
        if !self.tried {
            self.tried = true;
            self.x11 = (|| {
                let (conn, _) = x11rb::connect(None).ok()?;
                let setup = conn.setup();
                let (min, max) = (setup.min_keycode, setup.max_keycode);
                let map = conn.get_keyboard_mapping(min, max - min + 1).ok()?.reply().ok()?;
                let per = map.keysyms_per_keycode as usize;
                // keysym «v» — 0x76
                let i = map.keysyms.chunks(per.max(1)).position(|k| k.first() == Some(&0x76))?;
                Some((conn, min + i as u8))
            })();
        }
        let (conn, code) = self.x11.as_ref()?;
        if !ctrl {
            return Some(false);
        }
        let keys = conn.query_keymap().ok()?.reply().ok()?.keys;
        Some(keys[*code as usize / 8] & (1 << (code % 8)) != 0)
    }

    #[cfg(target_os = "windows")]
    fn down(&mut self, ctrl: bool) -> Option<bool> {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(key: i32) -> i16;
        }
        // 0x56 — клавиша V
        // SAFETY: функция без указателей, только читает состояние клавиши.
        Some(ctrl && unsafe { GetAsyncKeyState(0x56) } as u16 & 0x8000 != 0)
    }

    #[cfg(not(any(xdg, target_os = "windows")))]
    fn down(&mut self, _ctrl: bool) -> Option<bool> {
        None
    }
}

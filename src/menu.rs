use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

const ITEM_H: u16 = 32;
const MENU_W: u16 = 140;
const FONT_SIZE: f64 = 13.0;
const PAD_X: f64 = 16.0;

pub struct MenuItem {
    pub label: &'static str,
    pub id: &'static str,
}

pub struct ContextMenu {
    pub win: Window,
    gc: Gcontext,
    depth: u8,
    pub items: Vec<MenuItem>,
    hovered: Option<usize>,
    font_face: Option<cairo::FontFace>,
}

impl ContextMenu {
    pub fn new(
        conn: &RustConnection,
        screen: &x11rb::protocol::xproto::Screen,
        depth: u8,
        visual_id: u32,
        colormap: u32,
        items: Vec<MenuItem>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let h = ITEM_H * items.len() as u16 + 2;
        let win = conn.generate_id()?;
        conn.create_window(
            depth, win, screen.root,
            -1000, -1000, MENU_W, h, 0,
            WindowClass::INPUT_OUTPUT, visual_id,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION | EventMask::LEAVE_WINDOW),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        let font_face = init_font_face();
        Ok(Self { win, gc, depth, items, hovered: None, font_face })
    }

    pub fn show(&self, conn: &RustConnection, x: i16, y: i16) -> Result<(), Box<dyn std::error::Error>> {
        conn.configure_window(self.win, &ConfigureWindowAux::new().x(x as i32).y(y as i32))?;
        conn.map_window(self.win)?;
        conn.flush()?;
        self.render(conn)?;
        Ok(())
    }

    pub fn hide(&self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        conn.unmap_window(self.win)?;
        conn.flush()?;
        Ok(())
    }

    pub fn set_hovered(&mut self, conn: &RustConnection, idx: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
        if self.hovered != idx {
            self.hovered = idx;
            self.render(conn)?;
        }
        Ok(())
    }

    pub fn hit_test(&self, y: i16) -> Option<usize> {
        let idx = (y as usize) / ITEM_H as usize;
        if idx < self.items.len() { Some(idx) } else { None }
    }

    pub fn render(&self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        let h = ITEM_H as usize * self.items.len() + 2;
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, MENU_W as i32, h as i32)?;
        let ctx = cairo::Context::new(&surface)?;
        ctx.set_source_rgba(0.10, 0.10, 0.13, 0.96);
        rounded_rect(&ctx, 0.0, 0.0, MENU_W as f64, h as f64, 8.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        ctx.set_line_width(1.0);
        rounded_rect(&ctx, 0.5, 0.5, MENU_W as f64 - 1.0, h as f64 - 1.0, 8.0);
        ctx.stroke()?;
        if let Some(ref face) = self.font_face { ctx.set_font_face(face); }
        ctx.set_font_size(FONT_SIZE);
        for (i, item) in self.items.iter().enumerate() {
            let iy = (i * ITEM_H as usize) as f64 + 1.0;
            if self.hovered == Some(i) {
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
                rounded_rect(&ctx, 4.0, iy + 2.0, MENU_W as f64 - 8.0, ITEM_H as f64 - 4.0, 5.0);
                ctx.fill()?;
            }
            let is_destructive = item.id == "quit" || item.id == "close";
            ctx.set_source_rgba(1.0, if is_destructive { 0.4 } else { 1.0 }, if is_destructive { 0.4 } else { 1.0 }, 0.95);
            let ext = ctx.text_extents(item.label)
                .unwrap_or_else(|_| cairo::TextExtents::new(0.0,0.0,0.0,FONT_SIZE,0.0,0.0));
            ctx.move_to(PAD_X, iy + (ITEM_H as f64 + ext.height()) / 2.0 - ext.y_bearing() - ext.height());
            let _ = ctx.show_text(item.label);
            if i + 1 < self.items.len() {
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
                ctx.set_line_width(0.5);
                ctx.move_to(8.0, iy + ITEM_H as f64 - 0.5);
                ctx.line_to(MENU_W as f64 - 8.0, iy + ITEM_H as f64 - 0.5);
                ctx.stroke()?;
            }
        }
        drop(ctx);
        let data = surface.take_data().map_err(|_| "surface error")?;
        let stride = (MENU_W as usize) * 4;
        let chunk_rows: usize = (65536 / stride).max(1);
        let mut y_off = 0i16;
        while (y_off as usize) < h {
            let rows = (chunk_rows as i16).min(h as i16 - y_off) as u16;
            let start = y_off as usize * stride;
            let end = start + rows as usize * stride;
            conn.put_image(
                ImageFormat::Z_PIXMAP, self.win, self.gc,
                MENU_W, rows, 0, y_off, 0, self.depth,
                &data[start..end],
            )?;
            y_off += rows as i16;
        }
        conn.flush()?;
        Ok(())
    }
}

fn rounded_rect(ctx: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    ctx.new_sub_path();
    ctx.arc(x + r,     y + r,     r, std::f64::consts::PI, 3.0 * std::f64::consts::PI / 2.0);
    ctx.arc(x + w - r, y + r,     r, 3.0 * std::f64::consts::PI / 2.0, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::PI / 2.0);
    ctx.arc(x + r,     y + h - r, r, std::f64::consts::PI / 2.0, std::f64::consts::PI);
    ctx.close_path();
}

fn init_font_face() -> Option<cairo::FontFace> {
    const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/NotoSansCJK-Regular.ttc");
    let lib = freetype::Library::init().ok()?;
    let ft = lib.new_memory_face(FONT_DATA.to_vec(), 2)
        .or_else(|_| lib.new_memory_face(FONT_DATA.to_vec(), 0)).ok()?;
    cairo::FontFace::create_from_ft(&ft).ok()
}

pub const SETTINGS_W: u16 = 320;
const SETTINGS_ROW_H: u16 = 44;
const SETTINGS_PAD: f64 = 16.0;
const SETTINGS_FONT: f64 = 13.0;

pub struct CharacterEntry {
    pub id: String,
    pub name: String,
    pub avatar_path: String,
}

pub struct AnimationEntry {
    pub id: String,
    pub name: String,
    pub kind_str: String,
    pub path: String,
}

pub struct SettingsWindow {
    pub win: Window,
    gc: Gcontext,
    depth: u8,
    pub characters: Vec<CharacterEntry>,
    pub animations: Vec<AnimationEntry>,
    pub selected: usize,
    pub selected_anim: usize,
    pub scale: f64,
    pub dragging_scale: bool,
    pub tab: u8,
    font_face: Option<cairo::FontFace>,
    pub visible: bool,
}

fn char_window_h(n_chars: usize) -> u16 {
    let chars_h = 8 + n_chars as u16 * SETTINGS_ROW_H + 16;
    chars_h.max(240)
}

fn anim_window_h(n_anims: usize) -> u16 {
    let row_h: u16 = 36;
    let h = 8 + n_anims as u16 * (row_h + 2) + 80;
    h.max(240)
}

impl SettingsWindow {
    pub fn new(
        conn: &RustConnection,
        screen: &Screen,
        depth: u8,
        visual_id: u32,
        colormap: u32,
        characters: Vec<CharacterEntry>,
        animations: Vec<AnimationEntry>,
        selected: usize,
        selected_anim: usize,
        scale: f64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let h = Self::total_h(characters.len(), animations.len(), 0);
        let win = conn.generate_id()?;
        conn.create_window(
            depth, win, screen.root,
            -1000, -1000, SETTINGS_W, h, 0,
            WindowClass::INPUT_OUTPUT, visual_id,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::POINTER_MOTION | EventMask::BUTTON_RELEASE | EventMask::LEAVE_WINDOW),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        let font_face = init_font_face();
        Ok(Self { win, gc, depth, characters, animations, selected, selected_anim, scale, dragging_scale: false, tab: 0, font_face, visible: false })
    }

    fn total_h(n_chars: usize, n_anims: usize, tab: u8) -> u16 {
        if tab == 0 {
            let h = char_window_h(n_chars);
            (SETTINGS_PAD as u16 + 120).max(h)
        } else {
            let h = anim_window_h(n_anims);
            (SETTINGS_PAD as u16 + 120).max(h)
        }
    }

    pub fn show(&mut self, conn: &RustConnection, x: i16, y: i16, selected: usize, selected_anim: usize, scale: f64, screen_w: i16, screen_h: i16) -> Result<(), Box<dyn std::error::Error>> {
        self.selected = selected;
        self.selected_anim = selected_anim;
        self.scale = scale;
        self.visible = true;
        let h = Self::total_h(self.characters.len(), self.animations.len(), self.tab);
        let mut px = x as i32;
        let mut py = y as i32;
        if py + h as i32 > screen_h as i32 { py = (y as i32 - h as i32 - 8).max(4); }
        if px + SETTINGS_W as i32 > screen_w as i32 { px = (screen_w as i32 - SETTINGS_W as i32 - 4).max(4); }
        if px < 4 { px = 4; }
        conn.configure_window(self.win, &ConfigureWindowAux::new()
            .x(px).y(py).height(h as u32).width(SETTINGS_W as u32))?;
        conn.map_window(self.win)?;
        conn.flush()?;
        self.render(conn)
    }

    pub fn hide(&mut self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        self.visible = false;
        conn.unmap_window(self.win)?;
        conn.flush()?;
        Ok(())
    }

    pub fn hit_test(&self, mx: f64, my: f64) -> SettingsHit {
        if mx >= 12.0 && mx <= 64.0 && my >= 10.0 && my <= 34.0 { return SettingsHit::Close; }
        let tab_y = 44.0f64;
        let tab_h = 34.0f64;
        if my >= tab_y && my <= tab_y + tab_h {
            return if mx < SETTINGS_W as f64 / 2.0 { SettingsHit::Tab(0) } else { SettingsHit::Tab(1) };
        }
        let cy = 44.0 + 34.0;
        if self.tab == 0 {
            let start_y = cy + 8.0;
            for (i, _) in self.characters.iter().enumerate() {
                let ry = start_y + i as f64 * SETTINGS_ROW_H as f64;
                if my >= ry && my <= ry + SETTINGS_ROW_H as f64 && mx >= 8.0 && mx <= SETTINGS_W as f64 - 8.0 {
                    return SettingsHit::SelectChar(i);
                }
            }
        } else {
            let start_y = cy + 8.0;
            let row_h = 36.0f64;
            for (i, _) in self.animations.iter().enumerate() {
                let ry = start_y + i as f64 * (row_h + 2.0);
                if my >= ry && my <= ry + row_h && mx >= 12.0 && mx <= SETTINGS_W as f64 - 12.0 {
                    return SettingsHit::SelectAnim(i);
                }
            }
            let slider_x = SETTINGS_PAD;
            let slider_y = start_y + self.animations.len() as f64 * (row_h + 2.0) + 16.0;
            let slider_w = SETTINGS_W as f64 - SETTINGS_PAD * 2.0;
            if my >= slider_y - 10.0 && my <= slider_y + 16.0 && mx >= slider_x && mx <= slider_x + slider_w {
                return SettingsHit::ScaleSlider((mx - slider_x) / slider_w);
            }
        }
        SettingsHit::None
    }

    pub fn render(&self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        let h = Self::total_h(self.characters.len(), self.animations.len(), self.tab);
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, SETTINGS_W as i32, h as i32)?;
        let ctx = cairo::Context::new(&surface)?;
        ctx.set_source_rgba(0.10, 0.10, 0.13, 0.97);
        rounded_rect(&ctx, 0.0, 0.0, SETTINGS_W as f64, h as f64, 10.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        ctx.set_line_width(1.0);
        rounded_rect(&ctx, 0.5, 0.5, SETTINGS_W as f64 - 1.0, h as f64 - 1.0, 10.0);
        ctx.stroke()?;
        if let Some(ref face) = self.font_face { ctx.set_font_face(face); }

        
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        ctx.set_font_size(SETTINGS_FONT + 2.0);
        draw_text_centered(&ctx, "宠物设置", SETTINGS_W as f64 / 2.0, 28.0);
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
        rounded_rect(&ctx, 12.0, 10.0, 44.0, 24.0, 6.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.8);
        ctx.set_font_size(SETTINGS_FONT);
        draw_text_centered(&ctx, "✕", 34.0, 26.0);

        
        let tab_y = 44.0f64;
        let tab_h = 34.0f64;
        for (i, label) in ["角色", "动画"].iter().enumerate() {
            let tx = i as f64 * SETTINGS_W as f64 / 2.0;
            let active = self.tab == i as u8;
            if active {
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
                ctx.rectangle(tx, tab_y, SETTINGS_W as f64 / 2.0, tab_h);
                ctx.fill()?;
            }
            ctx.set_source_rgba(1.0, 1.0, 1.0, if active { 0.95 } else { 0.45 });
            ctx.set_font_size(SETTINGS_FONT);
            draw_text_centered(&ctx, label, tx + SETTINGS_W as f64 / 4.0, tab_y + tab_h / 2.0 + 5.0);
            if active {
                ctx.set_source_rgba(0.4, 0.75, 1.0, 0.9);
                ctx.rectangle(tx + 12.0, tab_y + tab_h - 2.0, SETTINGS_W as f64 / 2.0 - 24.0, 2.0);
                ctx.fill()?;
            }
        }
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.05);
        ctx.rectangle(0.0, tab_y + tab_h, SETTINGS_W as f64, 1.0);
        ctx.fill()?;

        let cy = tab_y + tab_h;
        if self.tab == 0 {
            let start_y = cy + 8.0;
            for (i, entry) in self.characters.iter().enumerate() {
                let ry = start_y + i as f64 * SETTINGS_ROW_H as f64;
                let is_sel = i == self.selected;
                if is_sel {
                    ctx.set_source_rgba(0.3, 0.7, 1.0, 0.18);
                    rounded_rect(&ctx, 8.0, ry + 2.0, SETTINGS_W as f64 - 16.0, SETTINGS_ROW_H as f64 - 4.0, 6.0);
                    ctx.fill()?;
                }
                
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
                rounded_rect(&ctx, 12.0, ry + 6.0, 32.0, 32.0, 6.0);
                ctx.fill()?;
                if !entry.avatar_path.is_empty() {
                    if let Ok(img) = image::open(&entry.avatar_path) {
                        let thumb = img.resize_exact(32, 32, image::imageops::FilterType::Triangle);
                        let rgba = thumb.to_rgba8();
                        let mut bgra = Vec::with_capacity(32 * 32 * 4);
                        for px in rgba.pixels() { bgra.push(px[2]); bgra.push(px[1]); bgra.push(px[0]); bgra.push(px[3]); }
                        if let Ok(mut surf) = cairo::ImageSurface::create_for_data(bgra, cairo::Format::ARgb32, 32, 32, 128) {
                            ctx.save()?;
                            rounded_rect(&ctx, 12.0, ry + 6.0, 32.0, 32.0, 6.0);
                            ctx.clip();
                            ctx.set_source_surface(&surf, 12.0, ry + 6.0)?;
                            ctx.paint()?;
                            ctx.restore()?;
                            let _ = surf.finish();
                        }
                    }
                }
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                ctx.set_font_size(SETTINGS_FONT);
                let ext = ctx.text_extents(&entry.name).unwrap_or(cairo::TextExtents::new(0.0,0.0,0.0,0.0,0.0,0.0));
                ctx.move_to(50.0, ry + SETTINGS_ROW_H as f64 / 2.0 + ext.height() / 2.0 - ext.y_bearing());
                let _ = ctx.show_text(&entry.name);
                if is_sel {
                    ctx.set_source_rgba(0.3, 0.8, 1.0, 0.9);
                    ctx.set_font_size(SETTINGS_FONT + 1.0);
                    draw_text_centered(&ctx, "✓", SETTINGS_W as f64 - 24.0, ry + SETTINGS_ROW_H as f64 / 2.0 + 5.0);
                }
            }
        } else {
            let mut ay = cy + 8.0;
            let row_h = 36.0f64;
            for (i, anim) in self.animations.iter().enumerate() {
                let is_sel = i == self.selected_anim;
                if is_sel {
                    ctx.set_source_rgba(0.3, 0.7, 1.0, 0.18);
                } else {
                    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.05);
                }
                rounded_rect(&ctx, 12.0, ay, SETTINGS_W as f64 - 24.0, row_h - 2.0, 6.0);
                ctx.fill()?;
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                ctx.set_font_size(SETTINGS_FONT);
                ctx.move_to(24.0, ay + row_h / 2.0 + 4.0);
                let _ = ctx.show_text(&anim.name);
                ctx.set_source_rgba(1.0, 1.0, 1.0, 0.4);
                ctx.set_font_size(SETTINGS_FONT - 1.0);
                ctx.move_to(SETTINGS_W as f64 - 70.0, ay + row_h / 2.0 + 4.0);
                let _ = ctx.show_text(&anim.kind_str);
                if is_sel {
                    ctx.set_source_rgba(0.3, 0.8, 1.0, 0.9);
                    draw_text_centered(&ctx, "✓", SETTINGS_W as f64 - 20.0, ay + row_h / 2.0 + 4.0);
                }
                ay += row_h + 2.0;
            }
            
            ay += 16.0;
            let slider_x = SETTINGS_PAD;
            let slider_w = SETTINGS_W as f64 - SETTINGS_PAD * 2.0;
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.3);
            ctx.set_font_size(SETTINGS_FONT);
            ctx.move_to(SETTINGS_PAD, ay + 4.0);
            let _ = ctx.show_text("大小倍数");
            let slider_y = ay + 24.0;
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
            ctx.rectangle(slider_x, slider_y, slider_w, 6.0);
            ctx.fill()?;
            let knob_x = slider_x + (self.scale - 0.2) / 4.8 * slider_w;
            ctx.set_source_rgba(0.4, 0.75, 1.0, 0.9);
            ctx.arc(knob_x, slider_y + 3.0, 8.0, 0.0, std::f64::consts::PI * 2.0);
            ctx.fill()?;
            ctx.set_source_rgba(1.0, 1.0, 1.0, 0.7);
            ctx.set_font_size(SETTINGS_FONT - 1.0);
            draw_text_centered(&ctx, &format!("{:.1}x", self.scale), SETTINGS_W as f64 / 2.0, slider_y + 30.0);
        }

        drop(ctx);
        let data = surface.take_data().map_err(|_| "surface error")?;
        let stride = (SETTINGS_W as usize) * 4;
        let chunk_rows: usize = (65536 / stride).max(1);
        let mut y_off = 0i16;
        while (y_off as usize) < h as usize {
            let rows = (chunk_rows as i16).min(h as i16 - y_off) as u16;
            let start = y_off as usize * stride;
            let end = start + rows as usize * stride;
            conn.put_image(
                ImageFormat::Z_PIXMAP, self.win, self.gc,
                SETTINGS_W, rows, 0, y_off, 0, self.depth,
                &data[start..end],
            )?;
            y_off += rows as i16;
        }
        conn.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsHit {
    Close,
    Tab(u8),
    SelectChar(usize),
    SelectAnim(usize),
    ScaleSlider(f64),
    None,
}

fn draw_text_centered(ctx: &cairo::Context, text: &str, cx: f64, y: f64) {
    let ext = ctx.text_extents(text).unwrap_or(cairo::TextExtents::new(0.0,0.0,0.0,0.0,0.0,0.0));
    ctx.move_to(cx - ext.width() / 2.0, y);
    let _ = ctx.show_text(text);
}

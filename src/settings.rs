use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use std::ffi::CString;
use std::ptr;

const WIN_W: u16 = 500;
const WIN_H: u16 = 520;
const FONT: f64 = 12.0;
const LEFT_W: f64 = 130.0;
const RIGHT_X: f64 = LEFT_W + 1.0;
const RIGHT_W: f64 = WIN_W as f64 - RIGHT_X;
const CHAR_LIST_Y: f64 = 52.0;
const CHAR_ROW_H: f64 = 72.0;
const ANIM_LIST_Y: f64 = 60.0;
const ANIM_LIST_X: f64 = RIGHT_X + 8.0;
const ANIM_CARD_W: f64 = 64.0;
const ANIM_CARD_H: f64 = 72.0;
const ANIM_CARD_GAP: f64 = 6.0;
const ANIM_LIST_H: f64 = ANIM_CARD_H + 12.0;
const PREVIEW_Y: f64 = ANIM_LIST_Y + ANIM_LIST_H + 16.0;
const PREVIEW_H: f64 = SLIDER_Y - PREVIEW_Y - 16.0;
const SLIDER_Y: f64 = 440.0;
const SLIDER_X: f64 = RIGHT_X + 16.0;
const SLIDER_W: f64 = RIGHT_W - 32.0;
const SCALE_MIN: f64 = 0.3;
const SCALE_MAX: f64 = 10.0;

pub struct CharInfo {
    pub id: String,
    pub name: String,
    pub avatar_path: String,
    pub anim_ids: Vec<String>,
    pub anim_names: Vec<String>,
    pub anim_paths: Vec<String>,
    pub anim_ticks_per_frame: Vec<u32>,
}

pub struct SettingsWindow {
    pub win: Window,
    gc: Gcontext,
    depth: u8,
    pub chars: Vec<CharInfo>,
    pub sel_char: usize,
    pub sel_anim: usize,
    pub scale: f64,
    pub visible: bool,
    pub dragging_slider: bool,
    pub char_scroll: usize,
    pub anim_scroll: usize,
    font_face: Option<cairo::FontFace>,
    preview_key: String,
    preview_player: Option<crate::video::VideoPlayer>,
    preview_surface: Option<cairo::ImageSurface>,
    preview_frame_paths: Vec<String>,
    preview_frame_idx: usize,
    preview_last_frame_time: Option<std::time::Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Close,
    SelectChar(usize),
    SelectAnim(usize),
    Slider(f64),
    None,
}

impl SettingsWindow {
    pub fn new(
        conn: &RustConnection,
        screen: &Screen,
        depth: u8,
        visual_id: u32,
        colormap: u32,
        chars: Vec<CharInfo>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let win = conn.generate_id()?;
        conn.create_window(
            depth, win, screen.root,
            -1000, -1000, WIN_W, WIN_H, 0,
            WindowClass::INPUT_OUTPUT, visual_id,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0)
                .colormap(colormap)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::POINTER_MOTION
                    | EventMask::BUTTON_RELEASE | EventMask::LEAVE_WINDOW),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        let font_face = init_font_face();
        Ok(Self { win, gc, depth, chars, sel_char: 0, sel_anim: 0, scale: 1.0, visible: false, dragging_slider: false, char_scroll: 0, anim_scroll: 0, font_face, preview_key: String::new(), preview_player: None, preview_surface: None, preview_frame_paths: vec![], preview_frame_idx: 0, preview_last_frame_time: None })
    }

    pub fn show(&mut self, conn: &RustConnection, x: i16, y: i16, sc_w: i16, sc_h: i16) -> Result<(), Box<dyn std::error::Error>> {
        self.visible = true;
        let mut px = x as i32;
        let mut py = y as i32;
        if py + WIN_H as i32 > sc_h as i32 { py = (y as i32 - WIN_H as i32 - 8).max(4); }
        if px + WIN_W as i32 > sc_w as i32 { px = (sc_w as i32 - WIN_W as i32 - 4).max(4); }
        if px < 4 { px = 4; }
        conn.configure_window(self.win, &ConfigureWindowAux::new().x(px).y(py))?;
        conn.map_window(self.win)?;
        conn.flush()?;
        self.render(conn)
    }

    pub fn has_video_preview(&self) -> bool {
        self.preview_player.is_some()
    }

    pub fn has_frame_preview(&self) -> bool {
        !self.preview_frame_paths.is_empty()
    }

    pub fn hide(&mut self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        self.visible = false;
        conn.unmap_window(self.win)?;
        conn.flush()?;
        Ok(())
    }

    pub fn current_anim_count(&self) -> usize {
        self.chars.get(self.sel_char).map_or(0, |c| c.anim_ids.len())
    }

    pub fn scroll_char(&mut self, delta: i32) {
        if delta < 0 { self.char_scroll = self.char_scroll.saturating_sub(1); }
        else if self.chars.len() > 0 { self.char_scroll = (self.char_scroll + 1).min(self.chars.len().saturating_sub(1)); }
    }

    pub fn scroll_anim(&mut self, delta: i32) {
        if delta < 0 { self.anim_scroll = self.anim_scroll.saturating_sub(1); }
        else if self.current_anim_count() > 0 { self.anim_scroll = (self.anim_scroll + 1).min(self.current_anim_count().saturating_sub(1)); }
    }

    pub fn hit_test(&self, mx: f64, my: f64) -> Hit {
        if mx >= 8.0 && mx <= 60.0 && my >= 10.0 && my <= 38.0 { return Hit::Close; }
        if mx < LEFT_W && my >= CHAR_LIST_Y {
            let visible_chars = ((WIN_H as f64 - CHAR_LIST_Y) / CHAR_ROW_H).floor() as usize;
            for vi in 0..visible_chars {
                let idx = self.char_scroll + vi;
                if idx >= self.chars.len() { break; }
                let ry = CHAR_LIST_Y + vi as f64 * CHAR_ROW_H;
                if my >= ry && my <= ry + CHAR_ROW_H - 4.0 {
                    return Hit::SelectChar(idx);
                }
            }
        }
        if mx >= RIGHT_X && my >= ANIM_LIST_Y && my <= ANIM_LIST_Y + ANIM_CARD_H {
            let visible_anims = ((RIGHT_W - 16.0) / (ANIM_CARD_W + ANIM_CARD_GAP)).floor() as usize;
            for vi in 0..visible_anims {
                let idx = self.anim_scroll + vi;
                if idx >= self.current_anim_count() { break; }
                let ax = ANIM_LIST_X + vi as f64 * (ANIM_CARD_W + ANIM_CARD_GAP);
                if mx >= ax && mx <= ax + ANIM_CARD_W {
                    return Hit::SelectAnim(idx);
                }
            }
        }
        if mx >= SLIDER_X && mx <= SLIDER_X + SLIDER_W && my >= SLIDER_Y - 10.0 && my <= SLIDER_Y + 16.0 {
            return Hit::Slider((mx - SLIDER_X) / SLIDER_W);
        }
        Hit::None
    }

    pub fn render(&mut self, conn: &RustConnection) -> Result<(), Box<dyn std::error::Error>> {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, WIN_W as i32, WIN_H as i32)?;
        let ctx = cairo::Context::new(&surface)?;

        ctx.set_source_rgba(0.10, 0.10, 0.13, 0.97);
        rounded_rect(&ctx, 0.0, 0.0, WIN_W as f64, WIN_H as f64, 12.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        ctx.set_line_width(1.0);
        rounded_rect(&ctx, 0.5, 0.5, WIN_W as f64 - 1.0, WIN_H as f64 - 1.0, 12.0);
        ctx.stroke()?;

        if let Some(ref face) = self.font_face { ctx.set_font_face(face); }

        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
        rounded_rect(&ctx, 8.0, 10.0, 52.0, 26.0, 8.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.85);
        ctx.set_font_size(FONT);
        draw_centered(&ctx, "关闭", 34.0, 27.0);

        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        ctx.set_font_size(FONT + 3.0);
        draw_centered(&ctx, "宠物设置", WIN_W as f64 / 2.0, 28.0);

        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        ctx.rectangle(LEFT_W, 0.0, 1.0, WIN_H as f64);
        ctx.fill()?;

        ctx.save()?;
        ctx.rectangle(0.0, CHAR_LIST_Y - 4.0, LEFT_W, WIN_H as f64 - CHAR_LIST_Y + 4.0);
        ctx.clip();
        let visible_chars = ((WIN_H as f64 - CHAR_LIST_Y) / CHAR_ROW_H).floor() as usize;
        for vi in 0..visible_chars {
            let idx = self.char_scroll + vi;
            if idx >= self.chars.len() { break; }
            let ch = &self.chars[idx];
            let ry = CHAR_LIST_Y + vi as f64 * CHAR_ROW_H;
            let active = idx == self.sel_char;
            if active {
                ctx.set_source_rgba(0.3, 0.7, 1.0, 0.18);
                rounded_rect(&ctx, 4.0, ry, LEFT_W - 8.0, CHAR_ROW_H - 4.0, 8.0);
                ctx.fill()?;
            }
            draw_avatar(&ctx, &ch.avatar_path, 4.0 + (LEFT_W - 8.0 - 40.0) / 2.0, ry + 6.0, 40.0);
            ctx.set_source_rgba(1.0, 1.0, 1.0, if active { 0.95 } else { 0.7 });
            ctx.set_font_size(FONT - 1.0);
            let ext = ctx.text_extents(&ch.name).unwrap_or(cairo::TextExtents::new(0.0,0.0,0.0,0.0,0.0,0.0));
            ctx.move_to(4.0 + (LEFT_W - 8.0 - ext.width()) / 2.0, ry + 6.0 + 40.0 + 14.0);
            let _ = ctx.show_text(&ch.name);
        }
        ctx.restore()?;

        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.06);
        ctx.rectangle(RIGHT_X, ANIM_LIST_Y - 4.0, RIGHT_W, ANIM_LIST_H + 4.0);
        ctx.fill()?;
        ctx.save()?;
        ctx.rectangle(RIGHT_X, ANIM_LIST_Y - 4.0, RIGHT_W, ANIM_LIST_H + 4.0);
        ctx.clip();

        if let Some(ch) = self.chars.get(self.sel_char) {
            let visible_anims = ((RIGHT_W - 16.0) / (ANIM_CARD_W + ANIM_CARD_GAP)).floor() as usize;
            for vi in 0..visible_anims {
                let idx = self.anim_scroll + vi;
                if idx >= ch.anim_names.len() { break; }
                let name = &ch.anim_names[idx];
                let ax = ANIM_LIST_X + vi as f64 * (ANIM_CARD_W + ANIM_CARD_GAP);
                let active = idx == self.sel_anim;
                ctx.set_source_rgba(if active { 0.3 } else { 1.0 }, if active { 0.7 } else { 1.0 }, if active { 1.0 } else { 1.0 }, if active { 0.2 } else { 0.07 });
                rounded_rect(&ctx, ax, ANIM_LIST_Y, ANIM_CARD_W, ANIM_CARD_H, 6.0);
                ctx.fill()?;
                if active {
                    ctx.set_source_rgba(0.3, 0.85, 1.0, 0.6);
                    ctx.set_line_width(1.5);
                    rounded_rect(&ctx, ax, ANIM_LIST_Y, ANIM_CARD_W, ANIM_CARD_H, 6.0);
                    ctx.stroke()?;
                }
                let avatar_size = ANIM_CARD_W - 16.0;
                draw_avatar(&ctx, &ch.avatar_path, ax + 8.0, ANIM_LIST_Y + 4.0, avatar_size);
                ctx.set_source_rgba(1.0, 1.0, 1.0, if active { 1.0 } else { 0.65 });
                ctx.set_font_size(FONT - 2.0);
                let ext = ctx.text_extents(name).unwrap_or(cairo::TextExtents::new(0.0,0.0,0.0,0.0,0.0,0.0));
                ctx.move_to(ax + (ANIM_CARD_W - ext.width()) / 2.0, ANIM_LIST_Y + 4.0 + avatar_size + 12.0);
                let _ = ctx.show_text(name);
            }
        }
        ctx.restore()?;

        if let Some(ch) = self.chars.get(self.sel_char) {
            if let Some(anim_path) = ch.anim_paths.get(self.sel_anim).filter(|p| !p.is_empty()) {
                let anim_key = format!("{}:{}", ch.id, self.sel_anim);
                let is_video = anim_path.ends_with(".mp4") || anim_path.ends_with(".mkv") || anim_path.ends_with(".webm");
                let max_w = (RIGHT_W - 24.0) as u32;
                let max_h = (PREVIEW_H - 8.0) as u32;
                let preview_size = max_w.min(max_h);

                if self.preview_key != anim_key {
                    self.preview_key = anim_key;
                    self.preview_player = None;
                    self.preview_surface = None;
                    self.preview_frame_paths.clear();
                    self.preview_frame_idx = 0;
                    self.preview_last_frame_time = None;
                    if is_video {
                        self.preview_player = crate::video::VideoPlayer::open_fit(anim_path, preview_size);
                    } else {
                        let dir = std::path::Path::new(anim_path).parent().unwrap_or(std::path::Path::new("."));
                        let stem = std::path::Path::new(anim_path).file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        let base = stem.trim_end_matches(|c: char| c.is_ascii_digit());
                        if base.is_empty() || base == stem {
                            for ext in &["svg", "png", "jpg", "jpeg"] {
                                let p = dir.join(format!("{}.{}", stem, ext));
                                if p.exists() { self.preview_frame_paths.push(p.to_string_lossy().to_string()); break; }
                            }
                        } else {
                            for ext in &["svg", "png", "jpg", "jpeg"] {
                                let mut frames = Vec::new();
                                for i in 1..100 {
                                    let p = dir.join(format!("{}{}.{}", base, i, ext));
                                    if p.exists() { frames.push(p.to_string_lossy().to_string()); }
                                }
                                if !frames.is_empty() {
                                    self.preview_frame_paths = frames;
                                    break;
                                }
                            }
                        }
                    }
                }

                if is_video {
                    if let Some(ref mut vp) = self.preview_player {
                        let vw = vp.width as i32;
                        let vh = vp.height as i32;
                        if let Some(frame_data) = vp.next_frame() {
                            let sd_stride = cairo::Format::ARgb32.stride_for_width(vw as u32).unwrap_or(vw * 4);
                            if let Ok(mut surf) = cairo::ImageSurface::create(cairo::Format::ARgb32, vw, vh) {
                                let mut sd = surf.data().unwrap();
                                let row_bytes = vw as usize * 4;
                                for row in 0..vh as usize {
                                    let src = &frame_data[row * row_bytes..(row + 1) * row_bytes];
                                    let dst = &mut sd[row * sd_stride as usize..row * sd_stride as usize + row_bytes];
                                    for px in 0..vw as usize {
                                        let si = px * 4;
                                        dst[si]     = src[si + 2];
                                        dst[si + 1] = src[si + 1];
                                        dst[si + 2] = src[si];
                                        dst[si + 3] = src[si + 3];
                                    }
                                }
                                drop(sd);
                                self.preview_surface = Some(surf);
                            }
                        }
                    }
                } else {
                    if !self.preview_frame_paths.is_empty() {
                        let tpf = self.chars.get(self.sel_char)
                            .and_then(|c| c.anim_ticks_per_frame.get(self.sel_anim))
                            .copied()
                            .unwrap_or(6);
                        let frame_interval = std::time::Duration::from_millis(tpf as u64 * 40);
                        let now = std::time::Instant::now();
                        let should_advance = match self.preview_last_frame_time {
                            Some(last) => now.duration_since(last) >= frame_interval,
                            None => true,
                        };
                        if should_advance {
                            self.preview_frame_idx += 1;
                            let idx = self.preview_frame_idx % self.preview_frame_paths.len();
                            let path = &self.preview_frame_paths[idx];
                            if let Some(surf) = load_image_surface(path, preview_size as f64) {
                                self.preview_surface = Some(surf);
                            }
                            self.preview_last_frame_time = Some(now);
                        }
                    }
                }

                ctx.save()?;
                ctx.rectangle(RIGHT_X + 8.0, PREVIEW_Y, RIGHT_W - 16.0, PREVIEW_H);
                let _ = ctx.clip();
                ctx.set_source_rgba(0.06, 0.06, 0.08, 1.0);
                ctx.rectangle(RIGHT_X + 8.0, PREVIEW_Y, RIGHT_W - 16.0, PREVIEW_H);
                ctx.fill()?;
                if let Some(ref surf) = self.preview_surface {
                    let sw = surf.width() as f64;
                    let sh = surf.height() as f64;
                    let cx = RIGHT_X + 8.0 + (RIGHT_W - 16.0) / 2.0 - sw / 2.0;
                    let cy = PREVIEW_Y + (PREVIEW_H - sh) / 2.0;
                    ctx.set_source_surface(surf, cx, cy)?;
                    ctx.paint()?;
                }
                ctx.restore()?;
            } else {
                self.preview_key.clear();
                self.preview_player = None;
                self.preview_surface = None;
                self.preview_frame_paths.clear();
                self.preview_last_frame_time = None;
            }
        }

        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.3);
        ctx.set_font_size(FONT - 1.0);
        ctx.move_to(SLIDER_X, SLIDER_Y - 14.0);
        let _ = ctx.show_text("大小");
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.12);
        ctx.rectangle(SLIDER_X, SLIDER_Y, SLIDER_W, 6.0);
        ctx.fill()?;
        let knob_x = SLIDER_X + (self.scale - SCALE_MIN) / (SCALE_MAX - SCALE_MIN) * SLIDER_W;
        ctx.set_source_rgba(0.4, 0.75, 1.0, 0.9);
        ctx.arc(knob_x, SLIDER_Y + 3.0, 7.0, 0.0, std::f64::consts::PI * 2.0);
        ctx.fill()?;
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.6);
        ctx.set_font_size(FONT - 2.0);
        draw_centered(&ctx, &format!("{:.1}x", self.scale), WIN_W as f64 / 2.0, SLIDER_Y + 26.0);

        drop(ctx);
        let data = surface.take_data().map_err(|_| "surface error")?;
        let stride = (WIN_W as usize) * 4;
        let chunk_rows: usize = (65536 / stride).max(1);
        let mut y_off = 0i16;
        while (y_off as usize) < WIN_H as usize {
            let rows = (chunk_rows as i16).min(WIN_H as i16 - y_off) as u16;
            let start = y_off as usize * stride;
            let end = start + rows as usize * stride;
            conn.put_image(ImageFormat::Z_PIXMAP, self.win, self.gc, WIN_W, rows, 0, y_off, 0, self.depth, &data[start..end])?;
            y_off += rows as i16;
        }
        conn.flush()?;
        Ok(())
    }
}

fn video_thumbnail(path: &str, max_size: u32) -> Option<cairo::ImageSurface> {
    unsafe {
        use ffmpeg_sys_next as ff;
        let path_c = CString::new(path).ok()?;
        let mut fmt_ctx: *mut ff::AVFormatContext = ptr::null_mut();
        if ff::avformat_open_input(&mut fmt_ctx, path_c.as_ptr(), ptr::null(), ptr::null_mut()) < 0 {
            return None;
        }
        ff::avformat_find_stream_info(fmt_ctx, ptr::null_mut());
        let nb = (*fmt_ctx).nb_streams as usize;
        let streams = std::slice::from_raw_parts((*fmt_ctx).streams, nb);
        let si = streams.iter().position(|&s| {
            (*(*s).codecpar).codec_type == ff::AVMediaType::AVMEDIA_TYPE_VIDEO
        })? as i32;
        let codecpar = (*streams[si as usize]).codecpar;
        let codec = ff::avcodec_find_decoder((*codecpar).codec_id);
        if codec.is_null() { ff::avformat_close_input(&mut fmt_ctx); return None; }
        let codec_ctx = ff::avcodec_alloc_context3(codec);
        ff::avcodec_parameters_to_context(codec_ctx, codecpar);
        if ff::avcodec_open2(codec_ctx, codec, ptr::null_mut()) < 0 {
            ff::avcodec_free_context(&mut { codec_ctx });
            ff::avformat_close_input(&mut fmt_ctx);
            return None;
        }
        let src_w = (*codec_ctx).width as u32;
        let src_h = (*codec_ctx).height as u32;
        let src_fmt = (*codec_ctx).pix_fmt;
        let (out_w, out_h) = if src_w >= src_h {
            (max_size, (max_size as f64 * src_h as f64 / src_w as f64).round() as u32)
        } else {
            ((max_size as f64 * src_w as f64 / src_h as f64).round() as u32, max_size)
        };
        let out_w = out_w.max(1);
        let out_h = out_h.max(1);
        let sws_ctx = ff::sws_getContext(
            src_w as i32, src_h as i32, src_fmt,
            out_w as i32, out_h as i32, ff::AVPixelFormat::AV_PIX_FMT_BGRA,
            2, ptr::null_mut(), ptr::null_mut(), ptr::null(),
        );
        if sws_ctx.is_null() {
            ff::avcodec_free_context(&mut { codec_ctx });
            ff::avformat_close_input(&mut fmt_ctx);
            return None;
        }
        let frame = ff::av_frame_alloc();
        let pkt = ff::av_packet_alloc();
        let linesize = (out_w * 4) as i32;

        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, out_w as i32, out_h as i32).ok()?;

        let mut decoded = false;
        for _ in 0..100 {
            if ff::av_read_frame(fmt_ctx, pkt) < 0 { break; }
            if (*pkt).stream_index != si { ff::av_packet_unref(pkt); continue; }
            if ff::avcodec_send_packet(codec_ctx, pkt) < 0 { ff::av_packet_unref(pkt); continue; }
            ff::av_packet_unref(pkt);
            if ff::avcodec_receive_frame(codec_ctx, frame) == 0 {
                let mut rgba_buf = vec![0u8; (out_w * out_h * 4) as usize];
                let mut frame_rgba = *frame;
                frame_rgba.data[0] = rgba_buf.as_mut_ptr();
                frame_rgba.linesize[0] = linesize;
                frame_rgba.width = out_w as i32;
                frame_rgba.height = out_h as i32;
                frame_rgba.format = ff::AVPixelFormat::AV_PIX_FMT_BGRA as i32;
                ff::sws_scale(sws_ctx,
                    (*frame).data.as_ptr() as *const *const u8,
                    (*frame).linesize.as_ptr(),
                    0, src_h as i32,
                    frame_rgba.data.as_mut_ptr(),
                    frame_rgba.linesize.as_mut_ptr());
                ff::av_frame_unref(frame);

                let stride = surface.stride() as usize;
                let mut surf_data = surface.data().unwrap();
                let row_bytes = out_w as usize * 4;
                for row in 0..out_h as usize {
                    let src_row = &rgba_buf[row * row_bytes..(row + 1) * row_bytes];
                    let dst_row = &mut surf_data[row * stride..row * stride + row_bytes];
                    dst_row.copy_from_slice(src_row);
                }
                drop(surf_data);
                decoded = true;
                break;
            }
        }
        ff::av_frame_free(&mut { frame });
        ff::av_packet_free(&mut { pkt });
        ff::sws_freeContext(sws_ctx);
        ff::avcodec_free_context(&mut { codec_ctx });
        ff::avformat_close_input(&mut fmt_ctx);
        if decoded { Some(surface) } else { None }
    }
}

fn draw_avatar(ctx: &cairo::Context, path: &str, x: f64, y: f64, size: f64) {
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    rounded_rect(ctx, x, y, size, size, 6.0);
    let _ = ctx.fill();
    if path.is_empty() { return; }
    let Some(surf) = load_image_surface(path, size) else { return };
    ctx.save().ok();
    rounded_rect(ctx, x, y, size, size, 6.0);
    let _ = ctx.clip();
    let cx = x + (size - surf.width() as f64) / 2.0;
    let cy = y + (size - surf.height() as f64) / 2.0;
    ctx.set_source_surface(&surf, cx, cy).ok();
    ctx.paint().ok();
    ctx.restore().ok();
    let _ = surf.finish();
}

fn load_image_surface(path: &str, size: f64) -> Option<cairo::ImageSurface> {
    let p = std::path::Path::new(path);
    let ext = p.extension()?.to_str()?;
    if ext == "svg" {
        let data = std::fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let src_w = tree.size().width();
        let src_h = tree.size().height();
        let sc = (size as f32 / src_w).min(size as f32 / src_h);
        let out_w = (src_w * sc).round().max(1.0) as u32;
        let out_h = (src_h * sc).round().max(1.0) as u32;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(out_w, out_h)?;
        pixmap.fill(resvg::tiny_skia::Color::TRANSPARENT);
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(sc, sc), &mut pixmap.as_mut());
        let rgba = pixmap.data();
        let stride = cairo::Format::ARgb32.stride_for_width(out_w).ok()? as usize;
        let row_bytes = out_w as usize * 4;
        let mut bgra = vec![0u8; stride * out_h as usize];
        for row in 0..out_h as usize {
            let src = &rgba[row * row_bytes..(row + 1) * row_bytes];
            let dst = &mut bgra[row * stride..row * stride + row_bytes];
            for i in 0..out_w as usize {
                let r = src[i*4] as u32; let g = src[i*4+1] as u32;
                let b = src[i*4+2] as u32; let a = src[i*4+3] as u32;
                dst[i*4]   = ((b*a + 127) >> 8) as u8;
                dst[i*4+1] = ((g*a + 127) >> 8) as u8;
                dst[i*4+2] = ((r*a + 127) >> 8) as u8;
                dst[i*4+3] = a as u8;
            }
        }
        cairo::ImageSurface::create_for_data(bgra, cairo::Format::ARgb32, out_w as i32, out_h as i32, stride as i32).ok()
    } else if ext == "png" {
        let mut f = std::fs::File::open(path).ok()?;
        let surf = cairo::ImageSurface::create_from_png(&mut f).ok()?;
        let sw = surf.width() as f64;
        let sh = surf.height() as f64;
        let sc = (size / sw).min(size / sh);
        let out_w = (sw * sc).round() as u32;
        let out_h = (sh * sc).round() as u32;
        let scaled = cairo::ImageSurface::create(cairo::Format::ARgb32, out_w as i32, out_h as i32).ok()?;
        {
            let ctx2 = cairo::Context::new(&scaled).ok()?;
            ctx2.scale(sc, sc);
            ctx2.set_source_surface(&surf, 0.0, 0.0).ok();
            ctx2.paint().ok();
        }
        Some(scaled)
    } else {
        let img = image::open(path).ok()?;
        let (src_w, src_h) = (img.width(), img.height());
        let sc = (size as f32 / src_w as f32).min(size as f32 / src_h as f32);
        let out_w = ((src_w as f32 * sc).round() as u32).max(1);
        let out_h = ((src_h as f32 * sc).round() as u32).max(1);
        let thumb = img.resize(out_w, out_h, image::imageops::FilterType::Triangle);
        let rgba = thumb.to_rgba8();
        let row_bytes = out_w as usize * 4;
        let mut bgra = vec![0u8; row_bytes * out_h as usize];
        for (i, px) in rgba.pixels().enumerate() {
            let off = i * 4;
            bgra[off] = px[2]; bgra[off+1] = px[1]; bgra[off+2] = px[0]; bgra[off+3] = px[3];
        }
        cairo::ImageSurface::create_for_data(bgra, cairo::Format::ARgb32, out_w as i32, out_h as i32, row_bytes as i32).ok()
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

fn draw_centered(ctx: &cairo::Context, text: &str, cx: f64, y: f64) {
    let ext = ctx.text_extents(text).unwrap_or(cairo::TextExtents::new(0.0,0.0,0.0,0.0,0.0,0.0));
    ctx.move_to(cx - ext.width() / 2.0, y);
    let _ = ctx.show_text(text);
}

fn init_font_face() -> Option<cairo::FontFace> {
    const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/NotoSansCJK-Regular.ttc");
    let lib = freetype::Library::init().ok()?;
    let ft = lib.new_memory_face(FONT_DATA.to_vec(), 2)
        .or_else(|_| lib.new_memory_face(FONT_DATA.to_vec(), 0)).ok()?;
    cairo::FontFace::create_from_ft(&ft).ok()
}

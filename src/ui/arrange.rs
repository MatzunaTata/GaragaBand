//! Arrangement view: ruler, track headers and the timeline lanes.

use super::theme::*;
use super::widgets::{icon_badge, paint_meter};
use super::{App, Drag};
use crate::engine::{ClipSnap, Snapshot, TrackKind, TrackParams, TrackPreset, TrackSnap, PEAK_STEP};
use crate::i18n::tr;
use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, FontId, Key, Layout, Rect, RichText, Sense, Stroke, UiBuilder};

impl App {
    pub(super) fn arrange(&mut self, ui: &mut egui::Ui, snap: &Snapshot, edit: &mut [TrackParams]) {
        let beat_len = snap.sr as f64 * 60.0 / snap.tr.bpm as f64;
        let area = ui.available_rect_before_wrap();
        if ui.rect_contains_pointer(area) {
            let (z, dx) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.x));
            if z != 1.0 {
                self.zoom = (self.zoom * z).clamp(6.0, 220.0);
            }
            if dx != 0.0 {
                self.scroll = (self.scroll - dx / self.zoom).max(0.0);
            }
        }
        let lane_w = (area.width() - HEADER_W).max(50.0);
        let visible = lane_w / self.zoom;
        if snap.tr.playing && self.drag.is_none() {
            let pb = (snap.tr.pos as f64 / beat_len) as f32;
            if pb > self.scroll + visible * 0.92 || pb < self.scroll {
                self.scroll = (pb - visible * 0.08).max(0.0);
            }
        }

        // ruler
        let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        let corner = Rect::from_min_size(rr.min, vec2(HEADER_W - 6.0, 26.0));
        let mut cu = ui.new_child(UiBuilder::new().max_rect(corner).layout(Layout::left_to_right(Align::Center)));
        cu.label(RichText::new(tr("Tracks", "Spuren")).font(semibold(15.0)));
        cu.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("+").on_hover_text(tr("Zoom in (Ctrl + mouse wheel)", "Hineinzoomen (Strg + Mausrad)")).clicked() {
                self.zoom = (self.zoom * 1.3).min(220.0);
            }
            if ui.small_button("–").on_hover_text(tr("Zoom out", "Herauszoomen")).clicked() {
                self.zoom = (self.zoom / 1.3).max(6.0);
            }
            if ui
                .small_button("▶")
                .on_hover_text(tr("Scroll right (or Shift + mouse wheel)", "Nach rechts blättern (oder Umschalt + Mausrad)"))
                .clicked()
            {
                self.scroll += visible * 0.5;
            }
            if ui.small_button("◀").on_hover_text(tr("Scroll left", "Nach links blättern")).clicked() {
                self.scroll = (self.scroll - visible * 0.5).max(0.0);
            }
        });
        let ruler = Rect::from_min_max(pos2(rr.left() + HEADER_W, rr.top()), rr.max);
        let resp = ui.interact(ruler, ui.id().with("ruler"), Sense::click_and_drag());
        let (zoom, scroll) = (self.zoom, self.scroll);
        let x_of = move |b: f64| ruler.left() + ((b - scroll as f64) * zoom as f64) as f32;
        if resp.clicked() || resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                let b = (scroll as f64 + ((p.x - ruler.left()) / zoom) as f64).round().max(0.0);
                self.engine.lock().seek((b * beat_len) as i64);
            }
        }
        let pt = ui.painter_at(ruler);
        pt.rect_filled(ruler, 6.0, pal().panel);
        if snap.tr.loop_on {
            let (s, e) = (snap.tr.loop_start_bar as f64 * 4.0, (snap.tr.loop_start_bar + snap.tr.loop_bars) as f64 * 4.0);
            pt.rect_filled(
                Rect::from_min_max(pos2(x_of(s), ruler.top()), pos2(x_of(e), ruler.bottom())),
                6.0,
                mix(pal().panel, pal().amber, 0.42),
            );
        }
        let bar_px = zoom * 4.0;
        let step = if bar_px < 28.0 {
            4
        } else if bar_px < 60.0 {
            2
        } else {
            1
        };
        let first_bar = (scroll / 4.0) as i64;
        for bar in first_bar..first_bar + (visible / 4.0) as i64 + 2 {
            let x = x_of(bar as f64 * 4.0);
            pt.vline(x, ruler.y_range(), Stroke::new(1.0f32, pal().border));
            if bar % step == 0 {
                pt.text(
                    pos2(x + 4.0, ruler.center().y),
                    Align2::LEFT_CENTER,
                    format!("{}", bar + 1),
                    FontId::proportional(12.5),
                    pal().text,
                );
            }
        }
        let px = x_of(snap.tr.pos as f64 / beat_len);
        pt.add(egui::Shape::convex_polygon(
            vec![pos2(px - 6.0, ruler.top()), pos2(px + 6.0, ruler.top()), pos2(px, ruler.top() + 11.0)],
            pal().text,
            Stroke::NONE,
        ));

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, t) in snap.tracks.iter().enumerate() {
                self.track_row(ui, snap, t, &mut edit[i], beat_len);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("New track:", "Neue Spur:")).font(semibold(14.5)));
                for (label, preset, tip) in [
                    (
                        tr("🎤 Voice", "🎤 Stimme"),
                        TrackPreset::Voice,
                        tr("Vocals through the microphone (input 1)", "Gesang über das Mikrofon (Eingang 1)"),
                    ),
                    (
                        tr("🎸 Guitar", "🎸 Gitarre"),
                        TrackPreset::Guitar,
                        tr("Guitar plugged into the interface (input 2)", "Gitarre am Interface (Eingang 2)"),
                    ),
                    (
                        "🎹 Keyboard",
                        TrackPreset::Keys,
                        tr("Sounds you play on a MIDI keyboard", "Klänge, die du mit einem MIDI-Keyboard spielst"),
                    ),
                    (
                        tr("🥁 Drums", "🥁 Schlagzeug"),
                        TrackPreset::Drums,
                        tr("Drums played on pads or keys", "Schlagzeug über Pads oder Tasten"),
                    ),
                ] {
                    if ui.add(egui::Button::new(RichText::new(label).size(15.0)).min_size(vec2(120.0, 32.0))).on_hover_text(tip).clicked() {
                        self.engine.lock().add_track(TrackParams::preset(preset));
                    }
                }
            });
            ui.add_space(8.0);
        });
    }

    fn track_row(&mut self, ui: &mut egui::Ui, snap: &Snapshot, t: &TrackSnap, p: &mut TrackParams, beat_len: f64) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
        let hrect = Rect::from_min_size(rect.min, vec2(HEADER_W - 6.0, ROW_H - 3.0));
        let lrect = Rect::from_min_max(pos2(rect.left() + HEADER_W, rect.top()), pos2(rect.right(), rect.bottom() - 3.0));
        let selected = snap.selected == t.id;
        let col = track_color(p.icon);
        let hresp = ui.interact(hrect, ui.id().with(("hdr", t.id)), Sense::click());
        if hresp.clicked() {
            self.engine.lock().selected = t.id;
        }
        let fill = if selected { mix(pal().card, col, 0.2) } else { pal().card };
        ui.painter().rect_filled(hrect, 10.0, fill);
        if selected {
            ui.painter().rect_stroke(hrect, 10.0, Stroke::new(1.5f32, col));
        }
        // level meter
        let m = Rect::from_min_size(pos2(hrect.right() - 15.0, hrect.top() + 9.0), vec2(5.0, hrect.height() - 18.0));
        let lv = if p.kind == TrackKind::Audio && !snap.tr.playing { t.in_meter.max(t.meter) } else { t.meter };
        paint_meter(ui.painter(), m, lv, true);

        let inner = Rect::from_min_max(hrect.min + vec2(10.0, 7.0), hrect.max - vec2(26.0, 6.0));
        let mut ui2 = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::LEFT)));
        let ui2 = &mut ui2;
        ui2.horizontal(|ui| {
            icon_badge(ui, p.icon, 26.0);
            if self.rename == Some(t.id) {
                let r = ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(140.0));
                r.request_focus();
                if r.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                    self.rename = None;
                }
            } else {
                let r = ui
                    .add(egui::Label::new(RichText::new(&p.name).font(semibold(16.0))).sense(Sense::click()))
                    .on_hover_text(tr("Click: select track · double-click: rename", "Klick: Spur auswählen · Doppelklick: umbenennen"));
                if r.double_clicked() {
                    self.rename = Some(t.id);
                } else if r.clicked() {
                    self.engine.lock().selected = t.id;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("🗑").on_hover_text(tr("Delete track", "Spur löschen")).clicked() {
                    self.confirm_delete = Some(t.id);
                }
            });
        });
        ui2.horizontal(|ui| {
            ui.spacing_mut().slider_width = 84.0;
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add(egui::Slider::new(&mut p.volume, 0.0..=1.0).show_value(false)).on_hover_text(tr("Volume of this track", "Lautstärke dieser Spur"));
            ui.toggle_value(&mut p.mute, tr("Mute", "Stumm")).on_hover_text(tr("Mute this track", "Diese Spur stumm schalten"));
            ui.toggle_value(&mut p.solo, "Solo").on_hover_text(tr("Hear only this track", "Nur diese Spur hören"));
            if p.kind == TrackKind::Audio {
                ui.toggle_value(&mut p.monitor, "🎧")
                    .on_hover_text(tr("Monitoring: you hear yourself with all effects in your headphones.\nOnly use with headphones, otherwise it will feed back!", "Mithören: Du hörst dich selbst mit allen Effekten im Kopfhörer.\nNur mit Kopfhörern benutzen, sonst pfeift es!"));
                ui.toggle_value(&mut p.armed, "⏺")
                    .on_hover_text(tr("Always record this track – even when another track is selected\n(e.g. voice and guitar at the same time).", "Immer mit aufnehmen – auch wenn eine andere Spur ausgewählt ist\n(z.B. Stimme und Gitarre gleichzeitig)."));
            }
        });
        self.lane(ui, lrect, snap, t, beat_len, col);
    }

    fn lane(&mut self, ui: &mut egui::Ui, rect: Rect, snap: &Snapshot, t: &TrackSnap, beat_len: f64, col: Color32) {
        let resp = ui.interact(rect, ui.id().with(("lane", t.id)), Sense::click_and_drag());
        let pt = ui.painter_at(rect);
        let (zoom, scroll) = (self.zoom, self.scroll);
        let x_of = |b: f64| rect.left() + ((b - scroll as f64) * zoom as f64) as f32;
        let b_of = |x: f32| scroll as f64 + ((x - rect.left()) / zoom) as f64;
        pt.rect_filled(rect, 8.0, if snap.selected == t.id { pal().lane_selected } else { pal().lane });
        if snap.tr.loop_on {
            let (s, e) = (snap.tr.loop_start_bar as f64 * 4.0, (snap.tr.loop_start_bar + snap.tr.loop_bars) as f64 * 4.0);
            pt.rect_filled(
                Rect::from_min_max(pos2(x_of(s), rect.top()), pos2(x_of(e), rect.bottom())),
                0.0,
                mix(pal().lane, pal().amber, 0.08),
            );
        }
        let first = scroll.floor() as i64;
        for b in first..first + (rect.width() / zoom) as i64 + 2 {
            let bar = b % 4 == 0;
            if !bar && zoom < 16.0 {
                continue;
            }
            pt.vline(x_of(b as f64), rect.y_range(), Stroke::new(1.0f32, mix(pal().lane, pal().text, if bar { 0.17 } else { 0.08 })));
        }

        // audio clips
        let mut hit: Option<&ClipSnap> = None;
        let pointer = resp.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.hover_pos()));
        for c in &t.clips {
            let x0 = x_of(c.start as f64 / beat_len);
            let x1 = x_of((c.start + c.len as i64) as f64 / beat_len);
            if x1 < rect.left() || x0 > rect.right() {
                continue;
            }
            let cr = Rect::from_min_max(pos2(x0, rect.top() + 3.0), pos2(x1.max(x0 + 3.0), rect.bottom() - 3.0));
            let sel = self.sel_clip == Some((t.id, c.id));
            // clips are stronger on the light theme so the white waveform stays readable
            let strength = match (pal().dark, sel) {
                (true, false) => 0.5,
                (true, true) | (false, false) => 0.8,
                (false, true) => 1.0,
            };
            pt.rect_filled(cr, 7.0, mix(pal().lane, col, strength));
            if sel {
                pt.rect_stroke(cr, 7.0, Stroke::new(2.0f32, pal().text));
            }
            let mid = cr.center().y;
            let hh = cr.height() * 0.46;
            let spp = beat_len / zoom as f64 / PEAK_STEP as f64; // peak values per pixel
            let xa = x0.max(rect.left()) as i32;
            let xb = x1.min(rect.right()) as i32;
            for x in xa..xb {
                let i0 = ((x as f32 - x0) as f64 * spp) as usize;
                let i1 = (((x + 1) as f32 - x0) as f64 * spp).ceil() as usize;
                let mut pk = 0.0f32;
                for v in c.peaks.iter().take(i1.max(i0 + 1)).skip(i0) {
                    pk = pk.max(*v);
                }
                let h = (pk.powf(0.8) * hh * 1.3).clamp(0.5, hh);
                pt.vline(x as f32 + 0.5, mid - h..=mid + h, Stroke::new(1.0f32, Color32::from_white_alpha(210)));
            }
            if let Some(p) = pointer {
                if cr.contains(p) {
                    hit = Some(c);
                }
            }
        }

        // notes
        if !t.notes.is_empty() {
            let (mut lo, mut hi) = (127u8, 0u8);
            for n in t.notes.iter() {
                lo = lo.min(n.pitch);
                hi = hi.max(n.pitch);
            }
            let span = (hi - lo).max(6) as f32;
            let note_color = if pal().dark { col.linear_multiply(1.3) } else { mix(col, pal().text, 0.25) };
            for n in t.notes.iter() {
                let x0 = x_of(n.start);
                if x0 > rect.right() {
                    continue;
                }
                let w = ((n.len * zoom as f64) as f32).max(3.0);
                let y = rect.bottom() - 8.0 - (n.pitch - lo) as f32 / span * (rect.height() - 18.0);
                pt.rect_filled(Rect::from_min_size(pos2(x0, y - 2.0), vec2(w, 4.0)), 1.5, note_color);
            }
        } else if t.clips.is_empty() && !t.rec_active {
            let txt = match t.params.kind {
                TrackKind::Audio => {
                    tr("Still empty – select the track and press ⏺ Record", "Noch leer – Spur auswählen und ⏺ Aufnehmen drücken")
                }
                TrackKind::Instrument => {
                    tr("Still empty – press Record and play the keyboard", "Noch leer – Aufnehmen drücken und auf dem Keyboard spielen")
                }
                TrackKind::Drums => tr(
                    "Still empty – press Record and tap the pads (or insert a preset beat below)",
                    "Noch leer – Aufnehmen drücken und auf die Pads tippen (oder unten einen fertigen Beat einfügen)",
                ),
            };
            pt.text(
                pos2(rect.left() + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                txt,
                FontId::proportional(13.0),
                pal().text_dim.linear_multiply(0.75),
            );
        }

        // recording in progress
        if t.rec_active && snap.tr.precount == 0 {
            let pos_b = snap.tr.pos as f64 / beat_len;
            let mut start_b = snap.tr.rec_start as f64 / beat_len;
            if pos_b < start_b {
                start_b = snap.tr.loop_start_bar as f64 * 4.0;
            }
            let r = Rect::from_min_max(pos2(x_of(start_b), rect.top() + 3.0), pos2(x_of(pos_b), rect.bottom() - 3.0));
            pt.rect_filled(r, 7.0, pal().red.linear_multiply(0.45));
        }
        let px = x_of(snap.tr.pos as f64 / beat_len);
        pt.vline(px, rect.y_range(), Stroke::new(1.5f32, pal().text));

        // interaction
        if resp.drag_started() {
            if let (Some(c), Some(p)) = (hit, resp.interact_pointer_pos()) {
                self.engine.lock().push_undo();
                self.drag = Some(Drag { track: t.id, clip: c.id, orig: c.start, x0: p.x });
                self.sel_clip = Some((t.id, c.id));
            }
        }
        if resp.dragged() {
            if let (Some(d), Some(p)) = (&self.drag, resp.interact_pointer_pos()) {
                if d.track == t.id {
                    let db = ((p.x - d.x0) / zoom) as f64;
                    let orig_b = d.orig as f64 / beat_len;
                    let nb = ((orig_b + db) * 4.0).round() / 4.0;
                    let ns = ((nb - (orig_b * 4.0).round() / 4.0) * beat_len) as i64 + d.orig;
                    let mut e = self.engine.lock();
                    let cid = d.clip;
                    if let Some(track) = e.track_mut(t.id) {
                        if let Some(c) = track.clips.iter_mut().find(|c| c.id == cid) {
                            c.start = ns.max(0);
                        }
                    }
                }
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        if resp.clicked() {
            let mut e = self.engine.lock();
            e.selected = t.id;
            if let Some(c) = hit {
                self.sel_clip = Some((t.id, c.id));
            } else if let Some(p) = resp.interact_pointer_pos() {
                self.sel_clip = None;
                e.seek((b_of(p.x).round().max(0.0) * beat_len) as i64);
            }
        }
        if hit.is_some() {
            resp.on_hover_text(tr("Click: select · drag: move · Del: delete", "Klick: auswählen · Ziehen: verschieben · Entf: löschen"));
        }
    }
}

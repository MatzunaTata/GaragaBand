//! "Edit notes" tab: a simple piano roll / drum grid for the selected track.

use crate::dsp::autotune::note_name;
use crate::dsp::drums::{beat_hits, drum_name, BEATS};
use crate::engine::{Note, Snapshot, TrackKind, TrackSnap};
use crate::i18n::tr;
use crate::ui::theme::*;
use crate::ui::App;
use eframe::egui::{self, pos2, vec2, Align2, FontId, Rect, RichText, Sense, Stroke};
use std::sync::Arc;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn notes_tab(&mut self, ui: &mut egui::Ui, snap: &Snapshot, t: &TrackSnap) {
        if t.params.kind == TrackKind::Audio {
            ui.label(tr(
                "Notes only exist on keyboard and drum tracks. Select one of those above.",
                "Noten gibt es nur bei Keyboard- und Schlagzeug-Spuren. Wähle oben eine solche Spur aus.",
            ));
            return;
        }
        let drums = t.params.kind == TrackKind::Drums;
        let beat_len = snap.sr as f64 * 60.0 / snap.tr.bpm as f64;
        let tid = t.id;
        ui.horizontal(|ui| {
            if ui
                .button(tr("✨ Tighten timing", "✨ Timing glätten"))
                .on_hover_text(tr(
                    "Moves every note exactly onto the grid (quantize to 16ths)",
                    "Schiebt alle Noten exakt auf den Takt (Quantisieren auf 16tel)",
                ))
                .clicked()
            {
                let mut e = self.engine.lock();
                e.push_undo();
                if let Some(track) = e.track_mut(tid) {
                    for n in Arc::make_mut(&mut track.notes).iter_mut() {
                        n.start = (n.start * 4.0).round() / 4.0;
                    }
                }
            }
            if ui.button(tr("🗑 Delete all notes", "🗑 Alle Noten löschen")).clicked() {
                let mut e = self.engine.lock();
                e.push_undo();
                if let Some(track) = e.track_mut(tid) {
                    track.notes = Arc::new(vec![]);
                }
            }
            if drums {
                ui.menu_button(tr("🥁 Insert preset beat", "🥁 Fertigen Beat einfügen"), |ui| {
                    ui.label(
                        RichText::new(if snap.tr.loop_on {
                            tr("fills the loop region", "füllt den Loop-Bereich")
                        } else {
                            tr("4 bars from the playhead", "4 Takte ab der Abspielposition")
                        })
                        .small()
                        .weak(),
                    );
                    for beat in BEATS.iter() {
                        if ui.button(beat.name.get()).clicked() {
                            let (bar0, bars) = if snap.tr.loop_on {
                                (snap.tr.loop_start_bar as i64, snap.tr.loop_bars as i64)
                            } else {
                                ((snap.tr.pos as f64 / beat_len / 4.0) as i64, 4)
                            };
                            let mut e = self.engine.lock();
                            e.push_undo();
                            if let Some(track) = e.track_mut(tid) {
                                let notes = Arc::make_mut(&mut track.notes);
                                for b in bar0..bar0 + bars {
                                    for (tm, n, v) in beat_hits(beat) {
                                        notes.push(Note { start: b as f64 * 4.0 + tm, len: 0.25, pitch: n, vel: v });
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                    }
                });
            }
            ui.label(
                RichText::new(tr(
                    "Click an empty spot = add note · click a note = delete it",
                    "Klick auf freie Stelle = Note setzen · Klick auf Note = löschen",
                ))
                .weak(),
            );
        });

        // determine the rows
        let rows: Vec<u8> = if drums {
            let mut r = vec![49u8, 51, 46, 42, 48, 45, 41, 39, 37, 38, 36];
            for n in t.notes.iter() {
                if !r.contains(&n.pitch) {
                    r.push(n.pitch);
                }
            }
            r
        } else {
            let (mut lo, mut hi) = (55u8, 72u8);
            for n in t.notes.iter() {
                lo = lo.min(n.pitch.saturating_sub(2));
                hi = hi.max((n.pitch + 2).min(127));
            }
            (lo..=hi).rev().collect()
        };
        let avail = ui.available_size();
        let row_h = if drums { 15.0 } else { (avail.y / rows.len() as f32).clamp(9.0, 16.0) };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h * rows.len() as f32), Sense::click());
            let pt = ui.painter_at(rect);
            let grid = Rect::from_min_max(pos2(rect.left() + HEADER_W, rect.top()), rect.max);
            let (zoom, scroll) = (self.zoom, self.scroll);
            let x_of = |b: f64| grid.left() + ((b - scroll as f64) * zoom as f64) as f32;
            let col = track_color(t.params.icon);
            for (ri, pitch) in rows.iter().enumerate() {
                let y = rect.top() + ri as f32 * row_h;
                let black = !drums && matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
                let rr = Rect::from_min_size(pos2(grid.left(), y), vec2(grid.width(), row_h - 1.0));
                pt.rect_filled(rr, 0.0, if black { pal().lane } else { pal().lane_selected });
                let name = if drums { drum_name(*pitch).to_string() } else { note_name(*pitch as f32) };
                if drums || pitch % 12 == 0 || row_h > 13.0 {
                    pt.text(
                        pos2(grid.left() - 8.0, y + row_h * 0.5),
                        Align2::RIGHT_CENTER,
                        name,
                        FontId::proportional(11.0),
                        if pitch % 12 == 0 || drums { pal().text } else { pal().text_dim },
                    );
                }
            }
            let first = scroll.floor() as i64;
            for b in first..first + (grid.width() / zoom) as i64 + 2 {
                for q in 0..4 {
                    if q > 0 && zoom < 40.0 {
                        continue;
                    }
                    // sixteenth, bar and beat lines
                    let c = if q > 0 {
                        0.08
                    } else if b % 4 == 0 {
                        0.38
                    } else {
                        0.18
                    };
                    pt.with_clip_rect(grid).vline(
                        x_of(b as f64 + q as f64 * 0.25),
                        grid.y_range(),
                        Stroke::new(1.0f32, mix(pal().lane_selected, pal().text, c)),
                    );
                }
            }
            let gp = pt.with_clip_rect(grid);
            let mut hit_note: Option<usize> = None;
            let ptr = resp.interact_pointer_pos();
            for (ni, n) in t.notes.iter().enumerate() {
                let Some(ri) = rows.iter().position(|r| *r == n.pitch) else { continue };
                let r = Rect::from_min_size(
                    pos2(x_of(n.start), rect.top() + ri as f32 * row_h + 1.0),
                    vec2(((n.len * zoom as f64) as f32).max(5.0), row_h - 3.0),
                );
                gp.rect_filled(r, 2.0, mix(pal().lane, col, 0.45 + 0.55 * n.vel as f32 / 127.0));
                if ptr.is_some_and(|p| r.expand(1.0).contains(p)) {
                    hit_note = Some(ni);
                }
            }
            gp.vline(x_of(snap.tr.pos as f64 / beat_len), grid.y_range(), Stroke::new(1.5f32, pal().text));
            if resp.clicked() {
                if let Some(p) = ptr {
                    if p.x >= grid.left() {
                        let mut e = self.engine.lock();
                        e.push_undo();
                        if let Some(ni) = hit_note {
                            if let Some(track) = e.track_mut(tid) {
                                Arc::make_mut(&mut track.notes).remove(ni);
                            }
                        } else {
                            let ri = (((p.y - rect.top()) / row_h) as usize).min(rows.len() - 1);
                            let b = ((scroll as f64 + ((p.x - grid.left()) / zoom) as f64) * 4.0).floor() / 4.0;
                            let pitch = rows[ri];
                            if let Some(track) = e.track_mut(tid) {
                                Arc::make_mut(&mut track.notes).push(Note {
                                    start: b.max(0.0),
                                    len: if drums { 0.25 } else { 0.5 },
                                    pitch,
                                    vel: 100,
                                });
                            }
                            drop(e);
                            self.send_midi(0x90, pitch, 100);
                            self.preview_off.push((Instant::now() + Duration::from_millis(250), pitch));
                        }
                    }
                }
            }
        });
    }
}

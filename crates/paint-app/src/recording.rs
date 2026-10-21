//! Запись процесса рисования: включение, перенос рядом с файлом при сохранении, окно выгрузки
//! (без выбранных слоёв, с водяным знаком и паролем). Устройство записи — `paint_formats::record`.

use crate::i18n::{tr, trf};
use crate::{files, settings, PaintApp};
use eframe::egui;
use paint_formats::record::{self, Recorder};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// Окно «Выгрузить запись».
pub struct ExportDialog {
    rec: PathBuf,
    info: record::Info,
    exclude: HashSet<u64>,
    seconds: f32,
    fps: u32,
    max_side: u32,
    hold: f32,
    watermark: bool,
    password: String,
    show_password: bool,
}

/// Идущая выгрузка: доля готовности (×1000) и ответ.
pub struct Job {
    progress: Arc<AtomicU32>,
    done: Receiver<Result<String, String>>,
}

/// Запись рядом с рисунком: `рисунок.sai2` → `рисунок.sai2.gsrec`.
pub fn sidecar(file: &Path) -> PathBuf {
    let mut s = file.as_os_str().to_owned();
    s.push(".gsrec");
    PathBuf::from(s)
}

/// Запись для ещё не сохранённого рисунка — в папке настроек.
fn unsaved() -> PathBuf {
    let dir = settings::dir().unwrap_or_else(std::env::temp_dir).join("recordings");
    let _ = std::fs::create_dir_all(&dir);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    dir.join(format!("unsaved-{t}.gsrec"))
}

/// Перенести запись: нет файла на новом месте — переименовать, есть — дописать в его конец.
fn move_record(from: &Path, to: &Path) -> std::io::Result<()> {
    if !to.exists() {
        if std::fs::rename(from, to).is_ok() {
            return Ok(());
        }
        std::fs::copy(from, to)?;
    } else {
        let data = std::fs::read(from)?;
        let mut f = std::fs::OpenOptions::new().append(true).open(to)?;
        std::io::Write::write_all(&mut f, data.get(8..).unwrap_or_default())?;
    }
    std::fs::remove_file(from)
}

pub fn size_text(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.2} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

impl PaintApp {
    pub(crate) fn start_recording(&mut self) {
        if self.rec.is_some() {
            return;
        }
        let path = self.file.as_deref().map_or_else(unsaved, sidecar);
        self.start_recording_at(path);
    }

    fn start_recording_at(&mut self, path: PathBuf) {
        match Recorder::start(&path, &mut self.doc) {
            Ok(r) => {
                self.status = trf("Запись процесса: {0}", &[&path.display()]);
                self.rec = Some(r);
            }
            Err(e) => self.status = trf("Не удалось начать запись: {0}", &[&e]),
        }
    }

    /// Остановить запись. `by_user` — из меню: тогда при открытии рисунка она сама не продолжится.
    pub(crate) fn stop_recording_by(&mut self, by_user: bool) {
        if let Some(r) = self.rec.take() {
            let path = r.path.clone();
            match r.stop(&mut self.doc, by_user) {
                Ok(()) => self.status = trf("Запись остановлена: {0}", &[&path.display()]),
                Err(e) => self.status = trf("Сбой записи: {0}", &[&e]),
            }
        }
    }

    pub(crate) fn stop_recording(&mut self) {
        self.stop_recording_by(false);
    }

    /// После открытия рисунка: если рядом есть его запись — продолжить её.
    pub(crate) fn resume_recording(&mut self) {
        let Some(rec) = self.file.as_deref().map(sidecar).filter(|p| p.exists()) else { return };
        if self.rec.is_none() && record::info(&rec).is_ok_and(|i| !i.stopped) {
            self.start_recording();
            self.status = format!("{} — {}", self.status, tr("запись процесса продолжается"));
        }
    }

    /// После сохранения в `path`: запись переезжает к рисунку.
    pub(crate) fn recording_saved_as(&mut self, path: &Path) {
        let target = sidecar(path);
        if self.rec.as_ref().is_none_or(|r| r.path == target) {
            return;
        }
        let old = self.rec.as_ref().map(|r| r.path.clone()).unwrap_or_default();
        let status = self.status.clone();
        self.stop_recording();
        if let Err(e) = move_record(&old, &target) {
            self.status = trf("Не удалось перенести запись: {0}", &[&e]);
            return;
        }
        self.start_recording_at(target);
        self.status = status;
    }

    /// Раз в кадр: записать сделанные шаги, следить за выгрузкой.
    pub(crate) fn recording_tick(&mut self, ctx: &egui::Context) {
        if self.live.is_none() {
            if let Some(r) = self.rec.as_mut() {
                r.capture(&mut self.doc);
            }
        }
        if let Some(job) = &self.rec_job {
            match job.done.try_recv() {
                Ok(Ok(s)) => {
                    self.status = s;
                    self.rec_job = None;
                }
                Ok(Err(e)) => {
                    self.status = trf("Не удалось выгрузить запись: {0}", &[&e]);
                    self.rec_job = None;
                }
                Err(_) => {
                    let p = job.progress.load(Ordering::Relaxed) as f32 / 10.0;
                    self.status = trf("Выгрузка записи: {0} %", &[&format!("{p:.0}")]);
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
            }
        }
    }

    /// Файл записи текущего рисунка (идущей или лежащей рядом).
    fn record_file(&self) -> Option<PathBuf> {
        self.rec.as_ref().map(|r| r.path.clone()).or_else(|| self.file.as_deref().map(sidecar).filter(|p| p.exists()))
    }

    /// Подменю «Файл → Запись процесса».
    pub(crate) fn recording_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(tr("Запись процесса"), |ui| {
            if self.rec.is_none() {
                if ui.button(tr("Начать запись")).on_hover_text(tr("Каждый шаг рисования пишется в файл рядом с рисунком (.gsrec)")).clicked() {
                    self.start_recording();
                    ui.close_menu();
                }
            } else if ui.button(tr("Остановить запись")).clicked() {
                self.stop_recording_by(true);
                ui.close_menu();
            }
            let file = self.record_file();
            if ui.add_enabled(file.is_some() && self.rec_job.is_none(), egui::Button::new(tr("Выгрузить видео…"))).clicked() {
                if let Some(f) = file {
                    self.open_record_export(f);
                }
                ui.close_menu();
            }
        });
    }

    fn open_record_export(&mut self, rec: PathBuf) {
        if let Some(r) = self.rec.as_mut() {
            r.capture(&mut self.doc);
        }
        match record::info(&rec) {
            Ok(info) => {
                self.rec_export = Some(ExportDialog {
                    rec,
                    info,
                    exclude: HashSet::new(),
                    seconds: 30.0,
                    fps: 20,
                    max_side: 1280,
                    hold: 3.0,
                    watermark: self.watermark_on,
                    password: String::new(),
                    show_password: false,
                })
            }
            Err(e) => self.status = trf("Не удалось прочитать запись: {0}", &[&e]),
        }
    }

    pub(crate) fn record_export_window(&mut self, ctx: &egui::Context) {
        let Some(d) = self.rec_export.as_mut() else { return };
        let mut open = true;
        let mut go = false;
        egui::Window::new(tr("Выгрузить запись процесса")).open(&mut open).resizable(false).show(ctx, |ui| {
            let secs = d.info.last_ms.saturating_sub(d.info.first_ms) / 1000;
            ui.label(trf("Шагов: {0}, время записи: {1}, файл: {2}", &[&d.info.frames, &format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60), &size_text(d.info.bytes)]));
            ui.separator();
            ui.label(tr("Не показывать слои:"));
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for (uid, name) in &d.info.layers {
                    let mut off = d.exclude.contains(uid);
                    if ui.checkbox(&mut off, name).changed() {
                        if off {
                            d.exclude.insert(*uid);
                        } else {
                            d.exclude.remove(uid);
                        }
                    }
                }
            });
            ui.separator();
            ui.spacing_mut().slider_width = 180.0;
            ui.add(egui::Slider::new(&mut d.seconds, 5.0..=600.0).logarithmic(true).suffix(" s").text(tr("Длительность")));
            ui.add(egui::Slider::new(&mut d.fps, 5..=60).text(tr("Кадров в секунду")));
            ui.add(egui::Slider::new(&mut d.max_side, 256..=3840).suffix(" px").text(tr("Большая сторона кадра")));
            ui.add(egui::Slider::new(&mut d.hold, 0.0..=10.0).suffix(" s").text(tr("Показ готовой картинки в конце")));
            ui.checkbox(&mut d.watermark, tr("Водяной знак (настройки — «Файл → Водяной знак…»)"));
            ui.horizontal(|ui| {
                ui.label(tr("Пароль"));
                ui.add(egui::TextEdit::singleline(&mut d.password).password(!d.show_password).desired_width(160.0));
                ui.checkbox(&mut d.show_password, tr("показать"));
            });
            ui.weak(if d.password.is_empty() {
                tr("Без пароля — анимированный PNG (открывается в браузере).")
            } else {
                tr("С паролем — zip с шифрованием AES-256; открыть: 7-Zip, WinRAR, Keka (проводник Windows такой zip не откроет).")
            });
            ui.weak(tr("Сам файл записи (.gsrec) хранит все слои — передавайте только выгрузку."));
            if ui.button(tr("Выгрузить…")).clicked() {
                go = true;
            }
        });
        if !open {
            self.rec_export = None;
        }
        if go {
            let zip = self.rec_export.as_ref().is_some_and(|d| !d.password.is_empty());
            let path = self.record_export_default(zip);
            if !self.native_dialog(files::Ask::RecordExport(zip), &path) {
                self.run_record_export(path);
            }
        }
    }

    pub(crate) fn record_export_default(&self, zip: bool) -> PathBuf {
        let ext = if zip { "zip" } else { "png" };
        match &self.file {
            Some(f) => f.with_extension(format!("process.{ext}")),
            None => std::env::current_dir().unwrap_or_default().join(format!("grazysai-process.{ext}")),
        }
    }

    /// Выгрузить в `out` в своём потоке; окно закрывается.
    pub(crate) fn run_record_export(&mut self, out: PathBuf) {
        let Some(d) = self.rec_export.take() else { return };
        let opts = record::Export {
            exclude: d.exclude,
            max_side: d.max_side,
            seconds: d.seconds,
            fps: d.fps,
            hold: d.hold,
            watermark: d.watermark.then(|| self.watermark.clone()),
            password: (!d.password.is_empty()).then_some(d.password),
        };
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, rx) = channel();
        let p = progress.clone();
        let rec = d.rec;
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let r = record::export(&rec, &out, &opts, &|f| p.store((f * 1000.0) as u32, Ordering::Relaxed))
                .map(|n| trf("Запись выгружена: {0} (кадров: {1})", &[&out.display(), &n]))
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        self.rec_job = Some(Job { progress, done: rx });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_appends_to_existing_record() {
        let dir = std::env::temp_dir().join(format!("grazysai-mv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.gsrec"), dir.join("b.gsrec"));
        std::fs::write(&a, b"GSREC\x01\0\0AAAA").unwrap();
        std::fs::write(&b, b"GSREC\x01\0\0BBBB").unwrap();
        move_record(&a, &b).unwrap();
        assert_eq!(std::fs::read(&b).unwrap(), b"GSREC\x01\0\0BBBBAAAA");
        assert!(!a.exists());
        assert_eq!(sidecar(Path::new("x/рисунок.sai2")), PathBuf::from("x/рисунок.sai2.gsrec"));
        std::fs::remove_dir_all(dir).ok();
    }
}

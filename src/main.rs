#![windows_subsystem = "windows"]

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::System::Power::GetSystemPowerStatus;
use windows::Win32::System::Power::SYSTEM_POWER_STATUS;
use windows::Win32::System::Registry::{RegCloseKey, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ, REG_VALUE_TYPE};
use windows::core::PCWSTR;

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    date: String,
    duration_secs: u64,
    battery_used: i32,
    battery_start: u8,
    battery_end: u8,
}

#[derive(Clone, Debug)]
enum TrayAction {
    Show,
    Quit,
}

#[derive(Clone, Copy, PartialEq)]
enum Theme {
    Light,
    Dark,
}

struct BatteryApp {
    is_disconnected: bool,
    disconnected_at: Option<Instant>,
    battery_at_disconnect: u8,
    elapsed: Duration,
    battery_percent: u8,
    is_on_battery: bool,
    history: Vec<Session>,
    tray_rx: Arc<Mutex<mpsc::Receiver<TrayAction>>>,
    tray_icon: Option<TrayIcon>,
    autostart: bool,
    show_confirm_clear: bool,
    error_message: Option<String>,
    theme: Theme,
    show_graph: bool,
    low_battery_threshold: u8,
    mini_mode: bool,
    update_interval_secs: u64,
    sound_enabled: bool,
    update_check_enabled: bool,
    show_update_notification: bool,
    latest_version: Option<String>,
}

const REG_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "BatteryMonitor";
const HISTORY_FILE: &str = "battery_history.json";

impl BatteryApp {
    fn new(tray_rx: Arc<Mutex<mpsc::Receiver<TrayAction>>>) -> Self {
        let (is_on_battery, battery_percent) = query_power_status();
        let autostart = check_autostart();
        let history = load_history();
        Self {
            is_disconnected: false,
            disconnected_at: None,
            battery_at_disconnect: 0,
            elapsed: Duration::ZERO,
            battery_percent,
            is_on_battery,
            history,
            tray_rx,
            tray_icon: None,
            autostart,
            show_confirm_clear: false,
            error_message: None,
            theme: Theme::Light,
            show_graph: false,
            low_battery_threshold: 20,
            mini_mode: false,
            update_interval_secs: 1,
            sound_enabled: true,
            update_check_enabled: true,
            show_update_notification: false,
            latest_version: None,
        }
    }

    fn set_tray_icon(&mut self, icon: TrayIcon) {
        self.tray_icon = Some(icon);
    }

    fn update_tooltip(&self) {
        if let Some(ref tray) = self.tray_icon {
            let tooltip = if self.is_disconnected {
                format!(
                    "🔋 Monitor de Batería\n⚡ Desconectado | Batería: {}%\n⏱ Tiempo: {}",
                    self.battery_percent,
                    format_duration(self.elapsed.as_secs())
                )
            } else {
                format!(
                    "🔋 Monitor de Batería\n🔌 Conectado | Batería: {}%",
                    self.battery_percent
                )
            };
            let _ = tray.set_tooltip(Some(&tooltip));
        }
    }

    fn notify(&self, title: &str, message: &str) {
        if let Some(ref tray) = self.tray_icon {
            let tooltip = format!("{}: {}", title, message);
            let _ = tray.set_tooltip(Some(&tooltip));
        }
    }

    fn play_sound(&self, sound_type: &str) {
        if !self.sound_enabled {
            return;
        }
        unsafe {
            use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
            
            let sound_name: Vec<u16> = match sound_type {
                "disconnect" => "SystemHand".encode_utf16().chain(std::iter::once(0)).collect(),
                "connect" => "SystemAsterisk".encode_utf16().chain(std::iter::once(0)).collect(),
                "low_battery" => "SystemExclamation".encode_utf16().chain(std::iter::once(0)).collect(),
                _ => return,
            };
            
            PlaySoundW(
                windows::core::PCWSTR(sound_name.as_ptr()),
                None,
                SND_ALIAS | SND_ASYNC,
            );
        }
    }

    fn toggle_autostart(&mut self) {
        self.autostart = !self.autostart;
        if self.autostart {
            if enable_autostart() {
                self.error_message = None;
            } else {
                self.error_message = Some("Error al activar autostart".to_string());
                self.autostart = false;
            }
        } else {
            if disable_autostart() {
                self.error_message = None;
            } else {
                self.error_message = Some("Error al desactivar autostart".to_string());
                self.autostart = true;
            }
        }
    }

    fn save_history(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.history) {
            let _ = std::fs::write(HISTORY_FILE, json);
        }
    }

    fn update(&mut self) {
        if let Ok(rx) = self.tray_rx.lock() {
            while let Ok(action) = rx.try_recv() {
                match action {
                    TrayAction::Show => {}
                    TrayAction::Quit => {
                        std::process::exit(0);
                    }
                }
            }
        }

        let (on_battery, percent) = query_power_status();
        self.is_on_battery = on_battery;
        self.battery_percent = percent;

        if on_battery && !self.is_disconnected {
            self.is_disconnected = true;
            self.disconnected_at = Some(Instant::now());
            self.battery_at_disconnect = percent;
            self.notify("⚡ Desconectado", &format!("Batería al {}%. Contador iniciado.", percent));
            self.play_sound("disconnect");
        } else if !on_battery && self.is_disconnected {
            self.is_disconnected = false;
            if let Some(start) = self.disconnected_at {
                let duration = start.elapsed();
                let battery_used = self.battery_at_disconnect as i32 - percent as i32;
                let session = Session {
                    date: chrono_now(),
                    duration_secs: duration.as_secs(),
                    battery_used,
                    battery_start: self.battery_at_disconnect,
                    battery_end: percent,
                };
                self.history.insert(0, session);
                self.save_history();
                self.notify(
                    "🔌 Conectado de nuevo",
                    &format!("Tiempo: {} | Batería consumida: {}%", format_duration(duration.as_secs()), battery_used),
                );
                self.play_sound("connect");
            }
            self.disconnected_at = None;
        }

        if on_battery && percent <= self.low_battery_threshold && percent > 0 {
            self.notify("🪫 Batería baja", &format!("Queda {}% de batería", percent));
            self.play_sound("low_battery");
        }

        if self.is_disconnected {
            if let Some(start) = self.disconnected_at {
                self.elapsed = start.elapsed();
            }
        }

        self.update_tooltip();
    }
}

fn load_history() -> Vec<Session> {
    if let Ok(data) = std::fs::read_to_string(HISTORY_FILE) {
        if let Ok(history) = serde_json::from_str(&data) {
            return history;
        }
    }
    Vec::new()
}

fn query_power_status() -> (bool, u8) {
    unsafe {
        let mut status = SYSTEM_POWER_STATUS::default();
        if GetSystemPowerStatus(&mut status).is_ok() {
            let on_battery = status.ACLineStatus == 0;
            let percent = status.BatteryLifePercent;
            (on_battery, percent)
        } else {
            (false, 0)
        }
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let datetime = chrono::DateTime::from_timestamp(now as i64, 0).unwrap();
    datetime.format("%d/%m/%Y %H:%M").to_string()
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn check_autostart() -> bool {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let key_path: Vec<u16> = REG_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        if windows::Win32::System::Registry::RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            0,
            windows::Win32::System::Registry::KEY_READ,
            &mut hkey,
        ).is_ok() {
            let value_name: Vec<u16> = APP_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let mut data_type: REG_VALUE_TYPE = REG_VALUE_TYPE(0);
            let mut data_size: u32 = 0;
            let result = windows::Win32::System::Registry::RegQueryValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                None,
                Some(&mut data_type as *mut _),
                None,
                Some(&mut data_size),
            );
            RegCloseKey(hkey);
            result.is_ok()
        } else {
            false
        }
    }
}

fn enable_autostart() -> bool {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let key_path: Vec<u16> = REG_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        if windows::Win32::System::Registry::RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            0,
            None,
            windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
            windows::Win32::System::Registry::KEY_WRITE,
            None,
            &mut hkey,
            None,
        ).is_ok() {
            let exe_path = std::env::current_exe().unwrap_or_default();
            let value_data: Vec<u16> = format!("\"{}\"", exe_path.to_string_lossy()).encode_utf16().chain(std::iter::once(0)).collect();
            let value_name: Vec<u16> = APP_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_data: &[u8] = std::slice::from_raw_parts(
                value_data.as_ptr() as *const u8,
                value_data.len() * 2,
            );
            RegSetValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                0,
                REG_SZ,
                Some(byte_data),
            );
            RegCloseKey(hkey);
            true
        } else {
            false
        }
    }
}

fn disable_autostart() -> bool {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let key_path: Vec<u16> = REG_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        if windows::Win32::System::Registry::RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            0,
            windows::Win32::System::Registry::KEY_WRITE,
            &mut hkey,
        ).is_ok() {
            let value_name: Vec<u16> = APP_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
            RegCloseKey(hkey);
            true
        } else {
            false
        }
    }
}

fn check_for_updates() -> Option<String> {
    let repo_owner = "fvnks";
    let repo_name = "battery_monitor";
    
    let url = format!("https://api.github.com/repos/{}/{}/releases/latest", repo_owner, repo_name);
    
    match reqwest::blocking::get(&url) {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(json) = response.json::<serde_json::Value>() {
                    if let Some(tag) = json.get("tag_name").and_then(|v| v.as_str()) {
                        return Some(tag.to_string());
                    }
                }
            }
            None
        }
        Err(_) => None,
    }
}

fn create_tray_icon(tx: Sender<TrayAction>) -> tray_icon::Result<TrayIcon> {
    let icon_size: u32 = 64;
    let mut icon_data = vec![0u8; (icon_size * icon_size * 4) as usize];
    
    for y in 0..icon_size {
        for x in 0..icon_size {
            let idx = ((y * icon_size + x) * 4) as usize;
            let cx = x as f32 - 32.0;
            let cy = y as f32 - 32.0;
            let dist = (cx * cx + cy * cy).sqrt();
            
            if dist > 28.0 && dist < 32.0 {
                icon_data[idx] = 0;
                icon_data[idx + 1] = 123;
                icon_data[idx + 2] = 255;
                icon_data[idx + 3] = 255;
            }
            else if dist <= 28.0 {
                let t = (y as f32) / icon_size as f32;
                icon_data[idx] = (40.0 + t * 20.0) as u8;
                icon_data[idx + 1] = (167.0 - t * 50.0) as u8;
                icon_data[idx + 2] = (69.0 + t * 180.0) as u8;
                icon_data[idx + 3] = 255;
            }
        }
    }

    let icon = Icon::from_rgba(icon_data, icon_size, icon_size)
        .map_err(|e| tray_icon::Error::OsError(std::io::Error::other(e.to_string())))?;

    let menu = Menu::new();
    let show_item = MenuItem::new("📖 Mostrar", true, None);
    let quit_item = MenuItem::new("❌ Salir", true, None);
    let show_id = show_item.id().0.clone();
    let quit_id = quit_item.id().0.clone();
    menu.append(&show_item).map_err(|e| tray_icon::Error::OsError(std::io::Error::other(e.to_string())))?;
    menu.append(&quit_item).map_err(|e| tray_icon::Error::OsError(std::io::Error::other(e.to_string())))?;

    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("🔋 Monitor de Batería")
        .with_menu(Box::new(menu))
        .build()?;

    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id.0 == show_id {
            let _ = menu_tx.send(TrayAction::Show);
        } else if event.id.0 == quit_id {
            let _ = menu_tx.send(TrayAction::Quit);
        }
    }));

    Ok(tray)
}

impl eframe::App for BatteryApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update();

        ctx.request_repaint_after(Duration::from_secs(self.update_interval_secs));

        // Aplicar tema
        let visuals = if self.theme == Theme::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        ctx.set_visuals(visuals);

        // Colores según tema
        let (bg_color, card_color, text_primary, text_secondary, border_color) = if self.theme == Theme::Dark {
            (
                egui::Color32::from_rgb(30, 30, 30),
                egui::Color32::from_rgb(45, 45, 45),
                egui::Color32::from_rgb(230, 230, 230),
                egui::Color32::from_rgb(150, 150, 150),
                egui::Color32::from_rgb(60, 60, 60),
            )
        } else {
            (
                egui::Color32::from_rgb(245, 247, 250),
                egui::Color32::WHITE,
                egui::Color32::from_rgb(33, 37, 41),
                egui::Color32::from_rgb(108, 117, 125),
                egui::Color32::from_rgb(222, 226, 230),
            )
        };

        let primary = egui::Color32::from_rgb(0, 123, 255);
        let success = egui::Color32::from_rgb(40, 167, 69);
        let danger = egui::Color32::from_rgb(220, 53, 69);
        let warning = egui::Color32::from_rgb(255, 193, 7);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(bg_color))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);

                    // Header
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("🔋")
                                .size(48.0)
                                .color(primary),
                        );
                        ui.add_space(15.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Battery Monitor")
                                    .size(24.0)
                                    .strong()
                                    .color(text_primary),
                            );
                            ui.label(
                                egui::RichText::new("Monitoreo inteligente de energía")
                                    .size(12.0)
                                    .color(text_secondary),
                            );
                        });
                    });

                    ui.add_space(25.0);

                    // Estado
                    let (status_icon, status_text, status_color, status_bg) = if self.is_disconnected {
                        ("⚡", "DESCONECTADO", danger, egui::Color32::from_rgb(255, 235, 238))
                    } else {
                        ("🔌", "CONECTADO", success, egui::Color32::from_rgb(232, 245, 233))
                    };

                    if self.mini_mode {
                        // Mini modo - vista compacta
                        egui::Frame::default()
                            .fill(status_bg)
                            .rounding(12.0)
                            .inner_margin(15.0)
                            .show(ui, |ui| {
                                ui.set_min_width(440.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(status_icon)
                                            .size(24.0)
                                            .color(status_color),
                                    );
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new(status_text)
                                            .size(14.0)
                                            .strong()
                                            .color(status_color),
                                    );
                                    ui.add_space(20.0);
                                    let timer_text = if self.is_disconnected {
                                        format_duration(self.elapsed.as_secs())
                                    } else {
                                        "--:--:--".to_string()
                                    };
                                    ui.label(
                                        egui::RichText::new(format!("⏱ {}", timer_text))
                                            .size(12.0)
                                            .monospace()
                                            .color(primary),
                                    );
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new(format!("🔋 {}%", self.battery_percent))
                                            .size(12.0)
                                            .strong()
                                            .color(if self.battery_percent > 50 { success } else if self.battery_percent > 20 { warning } else { danger }),
                                    );
                                });
                            });
                    } else {
                        // Modo normal
                        egui::Frame::default()
                            .fill(status_bg)
                            .rounding(16.0)
                            .inner_margin(25.0)
                            .show(ui, |ui| {
                                ui.set_min_width(440.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(status_icon)
                                            .size(48.0)
                                            .color(status_color),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new(status_text)
                                            .size(20.0)
                                            .strong()
                                            .color(status_color),
                                    );
                                });
                            });
                    }

                    ui.add_space(20.0);

                    // Grid stats
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.vertical(|ui| {
                            egui::Frame::default()
                                .fill(card_color)
                                .rounding(12.0)
                                .stroke(egui::Stroke::new(1.0_f32, border_color))
                                .inner_margin(20.0)
                                .show(ui, |ui| {
                                    ui.set_min_width(210.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("⏱ TIEMPO")
                                                .size(10.0)
                                                .strong()
                                                .color(text_secondary),
                                        );
                                        ui.add_space(8.0);
                                        let timer_text = if self.is_disconnected {
                                            format_duration(self.elapsed.as_secs())
                                        } else {
                                            "--:--:--".to_string()
                                        };
                                        ui.label(
                                            egui::RichText::new(timer_text)
                                                .size(22.0)
                                                .strong()
                                                .monospace()
                                                .color(primary),
                                        );
                                    });
                                });
                            
                            ui.add_space(10.0);
                            
                            egui::Frame::default()
                                .fill(card_color)
                                .rounding(12.0)
                                .stroke(egui::Stroke::new(1.0_f32, border_color))
                                .inner_margin(20.0)
                                .show(ui, |ui| {
                                    ui.set_min_width(210.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("🔋 BATERÍA")
                                                .size(10.0)
                                                .strong()
                                                .color(text_secondary),
                                        );
                                        ui.add_space(8.0);
                                        let battery_color = if self.battery_percent > 50 {
                                            success
                                        } else if self.battery_percent > 20 {
                                            warning
                                        } else {
                                            danger
                                        };
                                        ui.label(
                                            egui::RichText::new(format!("{}%", self.battery_percent))
                                                .size(22.0)
                                                .strong()
                                                .color(battery_color),
                                        );
                                    });
                                });
                        });
                    });

                    ui.add_space(20.0);

                    // Progress bar
                    egui::Frame::default()
                        .fill(card_color)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0_f32, border_color))
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.set_min_width(440.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Nivel de carga")
                                            .size(12.0)
                                            .color(text_secondary),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{}%", self.battery_percent))
                                                .size(12.0)
                                                .strong()
                                                .color(text_primary),
                                        );
                                    });
                                });
                                ui.add_space(10.0);
                                let progress = self.battery_percent as f32 / 100.0;
                                let bar_color = if self.battery_percent > 50 {
                                    success
                                } else if self.battery_percent > 20 {
                                    warning
                                } else {
                                    danger
                                };
                                ui.add(
                                    egui::ProgressBar::new(progress)
                                        .fill(bar_color)
                                        .desired_width(400.0)
                                        .desired_height(12.0)
                                        .rounding(6.0),
                                );
                            });
                        });

                    ui.add_space(25.0);

                    // Estadísticas resumidas
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("📊 ESTADÍSTICAS")
                                .size(12.0)
                                .strong()
                                .color(text_secondary),
                        );
                    });

                    ui.add_space(10.0);

                    egui::Frame::default()
                        .fill(card_color)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0_f32, border_color))
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.set_min_width(440.0);
                            
                            if self.history.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new("Sin datos suficientes")
                                            .size(12.0)
                                            .color(text_secondary),
                                    );
                                });
                            } else {
                                let total_sessions = self.history.len();
                                let avg_consumption: f64 = self.history.iter().map(|s| s.battery_used as f64).sum::<f64>() / total_sessions as f64;
                                let avg_duration: f64 = self.history.iter().map(|s| s.duration_secs as f64).sum::<f64>() / total_sessions as f64;
                                let longest_session = self.history.iter().map(|s| s.duration_secs).max().unwrap_or(0);
                                
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Sesiones")
                                                .size(10.0)
                                                .color(text_secondary),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("{}", total_sessions))
                                                .size(16.0)
                                                .strong()
                                                .color(primary),
                                        );
                                    });
                                    ui.add_space(30.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Consumo prom.")
                                                .size(10.0)
                                                .color(text_secondary),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("{:.1}%", avg_consumption))
                                                .size(16.0)
                                                .strong()
                                                .color(warning),
                                        );
                                    });
                                    ui.add_space(30.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Duración prom.")
                                                .size(10.0)
                                                .color(text_secondary),
                                        );
                                        ui.label(
                                            egui::RichText::new(format_duration(avg_duration as u64))
                                                .size(16.0)
                                                .strong()
                                                .color(success),
                                        );
                                    });
                                    ui.add_space(30.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new("Más larga")
                                                .size(10.0)
                                                .color(text_secondary),
                                        );
                                        ui.label(
                                            egui::RichText::new(format_duration(longest_session))
                                                .size(16.0)
                                                .strong()
                                                .color(danger),
                                        );
                                    });
                                });
                            }
                        });

                    ui.add_space(25.0);

                    // Gráfico de uso
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("📈 GRÁFICO DE USO")
                                .size(12.0)
                                .strong()
                                .color(text_secondary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(if self.show_graph { "Ocultar" } else { "Mostrar" }).clicked() {
                                self.show_graph = !self.show_graph;
                            }
                        });
                    });

                    ui.add_space(10.0);

                    if self.show_graph {
                        egui::Frame::default()
                            .fill(card_color)
                            .rounding(12.0)
                            .stroke(egui::Stroke::new(1.0_f32, border_color))
                            .inner_margin(15.0)
                            .show(ui, |ui| {
                                ui.set_min_width(440.0);
                                ui.set_min_height(200.0);
                                
                                if self.history.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new("Sin datos para mostrar")
                                                .size(12.0)
                                                .color(text_secondary),
                                        );
                                    });
                                } else {
                                    // Gráfico de línea - evolución del consumo
                                    let sessions: Vec<&Session> = self.history.iter().take(15).rev().collect();
                                    let max_battery = sessions.iter().map(|s| s.battery_start).max().unwrap_or(100);
                                    let min_battery = sessions.iter().map(|s| s.battery_end).min().unwrap_or(0);
                                    let range = (max_battery - min_battery).max(1) as f32;
                                    
                                    let (response, painter) = ui.allocate_painter(
                                        egui::vec2(400.0, 150.0),
                                        egui::Sense::hover(),
                                    );
                                    
                                    let rect = response.rect;
                                    let n = sessions.len();
                                    if n < 2 {
                                        ui.label("Se necesitan al menos 2 sesiones");
                                        return;
                                    }
                                    
                                    let step_x = rect.width() / (n - 1) as f32;
                                    
                                    // Dibujar línea de inicio de batería
                                    let points_start: Vec<egui::Pos2> = sessions.iter().enumerate().map(|(i, s)| {
                                        let x = rect.left() + i as f32 * step_x;
                                        let y = rect.bottom() - ((s.battery_start - min_battery) as f32 / range) * rect.height();
                                        egui::pos2(x, y)
                                    }).collect();
                                    
                                    // Dibujar línea de fin de batería
                                    let points_end: Vec<egui::Pos2> = sessions.iter().enumerate().map(|(i, s)| {
                                        let x = rect.left() + i as f32 * step_x;
                                        let y = rect.bottom() - ((s.battery_end - min_battery) as f32 / range) * rect.height();
                                        egui::pos2(x, y)
                                    }).collect();
                                    
                                    // Dibujar líneas
                                    painter.line_segment([points_start[0], points_start[n-1]], egui::Stroke::new(2.0, primary));
                                    painter.line_segment([points_end[0], points_end[n-1]], egui::Stroke::new(2.0, success));
                                    
                                    // Dibujar puntos
                                    for i in 0..n {
                                        painter.circle_filled(points_start[i], 4.0, primary);
                                        painter.circle_filled(points_end[i], 4.0, success);
                                    }
                                    
                                    // Leyenda
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("●").color(primary));
                                        ui.label(egui::RichText::new("Inicio").size(10.0).color(text_secondary));
                                        ui.add_space(15.0);
                                        ui.label(egui::RichText::new("●").color(success));
                                        ui.label(egui::RichText::new("Fin").size(10.0).color(text_secondary));
                                        ui.add_space(15.0);
                                        ui.label(egui::RichText::new("●").color(warning));
                                        ui.label(egui::RichText::new("Consumo").size(10.0).color(text_secondary));
                                    });
                                }
                            });
                    }

                    ui.add_space(25.0);

                    // Historial
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("📊 HISTORIAL DE SESIONES")
                                .size(12.0)
                                .strong()
                                .color(text_secondary),
                        );
                    });

                    ui.add_space(10.0);

                    egui::Frame::default()
                        .fill(card_color)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0_f32, border_color))
                        .inner_margin(15.0)
                        .show(ui, |ui| {
                            ui.set_min_width(440.0);
                            egui::ScrollArea::vertical()
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    if self.history.is_empty() {
                                        ui.add_space(30.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                egui::RichText::new("📭")
                                                    .size(32.0)
                                                    .color(text_secondary),
                                            );
                                            ui.add_space(5.0);
                                            ui.label(
                                                egui::RichText::new("Sin sesiones registradas")
                                                    .size(12.0)
                                                    .color(text_secondary),
                                            );
                                        });
                                        ui.add_space(30.0);
                                    } else {
                                        for (i, session) in self.history.iter().enumerate() {
                                            egui::Frame::default()
                                                .fill(if self.theme == Theme::Dark {
                                                    egui::Color32::from_rgb(35, 35, 35)
                                                } else {
                                                    egui::Color32::from_rgb(248, 249, 250)
                                                })
                                                .rounding(8.0)
                                                .inner_margin(12.0)
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(format!("{:03}", i + 1))
                                                                .strong()
                                                                .monospace()
                                                                .color(primary),
                                                        );
                                                        ui.add_space(15.0);
                                                        ui.vertical(|ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(egui::RichText::new("📅").size(12.0));
                                                                ui.add_space(5.0);
                                                                ui.label(
                                                                    egui::RichText::new(&session.date)
                                                                        .size(11.0)
                                                                        .color(text_secondary),
                                                                );
                                                            });
                                                            ui.add_space(3.0);
                                                            ui.horizontal(|ui| {
                                                                ui.label(egui::RichText::new("⏱").size(12.0));
                                                                ui.add_space(5.0);
                                                                ui.label(
                                                                    egui::RichText::new(format_duration(session.duration_secs))
                                                                        .size(11.0)
                                                                        .strong()
                                                                        .color(primary),
                                                                );
                                                                ui.add_space(15.0);
                                                                ui.label(egui::RichText::new("🔋").size(12.0));
                                                                ui.add_space(5.0);
                                                                ui.label(
                                                                    egui::RichText::new(format!("{}% → {}% ({}%)",
                                                                        session.battery_start,
                                                                        session.battery_end,
                                                                        session.battery_used
                                                                    ))
                                                                        .size(11.0)
                                                                        .color(text_primary),
                                                                );
                                                            });
                                                        });
                                                    });
                                                });
                                            ui.add_space(8.0);
                                        }
                                    }
                                });
                        });

                    ui.add_space(20.0);

                    // Configuración
                    egui::Frame::default()
                        .fill(card_color)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0_f32, border_color))
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.set_min_width(440.0);
                            
                            // Autostart
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("⚙️ Iniciar con Windows")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("La aplicación se iniciará automáticamente al encender el equipo")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut autostart = self.autostart;
                                if ui.checkbox(&mut autostart, "").changed() {
                                    self.autostart = autostart;
                                    self.toggle_autostart();
                                }
                            });

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Batería baja personalizable
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("🪫 Alerta de batería baja")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Umbral para mostrar alerta")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut threshold = self.low_battery_threshold;
                                if ui.add(egui::Slider::new(&mut threshold, 5..=50)).changed() {
                                    self.low_battery_threshold = threshold;
                                }
                                ui.label(
                                    egui::RichText::new(format!("{}%", threshold))
                                        .size(12.0)
                                        .strong()
                                        .color(warning),
                                );
                            });

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Mini modo
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("📱 Mini modo")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Vista compacta con información esencial")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut mini = self.mini_mode;
                                if ui.checkbox(&mut mini, "").changed() {
                                    self.mini_mode = mini;
                                }
                            });

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Sonido
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("🔊 Alerta sonora")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Reproducir sonido al desconectar/conectar")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut sound = self.sound_enabled;
                                if ui.checkbox(&mut sound, "").changed() {
                                    self.sound_enabled = sound;
                                }
                            });

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Intervalo de actualización
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("⏱ Intervalo de actualización")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Frecuencia de monitoreo")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut interval = self.update_interval_secs;
                                egui::ComboBox::from_id_source("interval_combo")
                                    .selected_text(format!("{}s", interval))
                                    .show_ui(ui, |ui| {
                                        for i in [1, 2, 5, 10, 30] {
                                            ui.selectable_value(&mut interval, i, format!("{}s", i));
                                        }
                                    });
                                if interval != self.update_interval_secs {
                                    self.update_interval_secs = interval;
                                }
                            });

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Actualizador
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("🔄 Actualizador")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Buscar actualizaciones al iniciar")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut update_check = self.update_check_enabled;
                                if ui.checkbox(&mut update_check, "").changed() {
                                    self.update_check_enabled = update_check;
                                }
                            });

                            // Botón de actualización manual
                            if self.update_check_enabled {
                                ui.add_space(10.0);
                                if ui.button("🔍 Buscar actualizaciones ahora").clicked() {
                                    self.show_update_notification = true;
                                    // Aquí iría la lógica de verificación de actualizaciones
                                }
                                
                                if self.show_update_notification {
                                    ui.add_space(5.0);
                                    if let Some(ref version) = self.latest_version {
                                        ui.colored_label(success, format!("✅ Versión actual: {}", version));
                                    } else {
                                        ui.colored_label(text_secondary, "🔍 Buscando actualizaciones...");
                                    }
                                }
                            }

                            ui.add_space(15.0);
                            ui.separator();
                            ui.add_space(15.0);

                            // Tema
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("🎨 Tema")
                                            .size(13.0)
                                            .strong()
                                            .color(text_primary),
                                    );
                                    ui.add_space(3.0);
                                    ui.label(
                                        egui::RichText::new("Cambiar entre modo claro y oscuro")
                                            .size(10.0)
                                            .color(text_secondary),
                                    );
                                });
                                ui.add_space(20.0);
                                let mut dark_mode = self.theme == Theme::Dark;
                                if ui.checkbox(&mut dark_mode, "").changed() {
                                    self.theme = if dark_mode { Theme::Dark } else { Theme::Light };
                                }
                            });
                        });

                    // Error message
                    if let Some(ref error) = self.error_message {
                        ui.add_space(10.0);
                        ui.colored_label(danger, format!("❌ {}", error));
                    }

                    ui.add_space(15.0);

                    // Botones
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        if ui
                            .button(
                                egui::RichText::new("🗑 Limpiar historial")
                                    .size(11.0),
                            )
                            .clicked()
                        {
                            if self.show_confirm_clear {
                                self.history.clear();
                                self.save_history();
                                self.show_confirm_clear = false;
                            } else {
                                self.show_confirm_clear = true;
                            }
                        }
                        
                        if self.show_confirm_clear {
                            ui.label(
                                egui::RichText::new("¿Confirmar?")
                                    .size(10.0)
                                    .color(danger),
                            );
                            if ui.button("Sí").clicked() {
                                self.history.clear();
                                self.save_history();
                                self.show_confirm_clear = false;
                            }
                            if ui.button("No").clicked() {
                                self.show_confirm_clear = false;
                            }
                        }
                    });

                    ui.add_space(15.0);

                    ui.label(
                        egui::RichText::new("💡 La aplicación sigue ejecutándose en la bandeja del sistema")
                            .size(10.0)
                            .weak()
                            .color(text_secondary),
                    );

                    ui.add_space(15.0);
                });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let (tx, rx) = mpsc::channel();
    let tray_rx = Arc::new(Mutex::new(rx));

    let tray_icon = create_tray_icon(tx).ok();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 700.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Battery Monitor",
        options,
        Box::new(move |_cc| {
            let mut app = BatteryApp::new(tray_rx);
            if let Some(icon) = tray_icon {
                app.set_tray_icon(icon);
            }
            // Verificar actualizaciones al iniciar
            if app.update_check_enabled {
                app.latest_version = check_for_updates();
            }
            Ok(Box::new(app))
        }),
    )
}

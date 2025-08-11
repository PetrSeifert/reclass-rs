use eframe::egui::{
    self,
    Context,
    ScrollArea,
};

use crate::re_class_app::app::AppSignature;
fn parse_hex_u64_local(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(stripped) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(stripped, 16).ok()
    } else {
        t.parse::<u64>().ok()
    }
}
use crate::re_class_app::ReClassGui;

impl ReClassGui {
    pub(super) fn signatures_window(&mut self, ctx: &Context) {
        egui::Window::new("Signatures")
            .open(&mut self.signatures_window_open)
            .resizable(true)
            .show(ctx, |ui| {
                let handle_opt = self.app.handle.clone();
                // Borrow signatures mutably only within a small scope to avoid conflicts
                let sigs_ptr: *mut Vec<AppSignature> = self.app.get_signatures_mut() as *mut _;

                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        let sigs_mut: &mut Vec<AppSignature> = unsafe { &mut *sigs_ptr };
                        sigs_mut.push(AppSignature::default());
                    }
                    // Auto-resolve every frame for immediate feedback
                    if let Some(handle) = handle_opt.as_ref() {
                        let sigs_mut: &mut Vec<AppSignature> = unsafe { &mut *sigs_ptr };
                        for s in sigs_mut.iter_mut() {
                            // Sanitize before building
                            let sanitized =
                                s.pattern.split_whitespace().collect::<Vec<_>>().join(" ");
                            if handle::ByteSequencePattern::parse(&sanitized).is_none() {
                                s.last_value = None;
                                s.last_error = Some("Invalid pattern".to_string());
                                continue;
                            }
                            // Use live values from buffers if they parse, otherwise fall back
                            let offset_use = parse_hex_u64_local(&s.offset_buf).unwrap_or(s.offset);
                            let inst_len_use =
                                parse_hex_u64_local(&s.rel_inst_len_buf).unwrap_or(s.rel_inst_len);
                            s.offset = offset_use;
                            s.rel_inst_len = inst_len_use;
                            let sig_def = if s.is_relative {
                                handle::Signature::relative_address(
                                    &s.name,
                                    &sanitized,
                                    offset_use,
                                    inst_len_use,
                                )
                            } else {
                                handle::Signature::offset(&s.name, &sanitized, offset_use)
                            };
                            match handle.resolve_signature(&s.module, &sig_def) {
                                Ok(value) => {
                                    s.last_value = Some(value);
                                    s.last_error = None;
                                }
                                Err(e) => {
                                    s.last_value = None;
                                    s.last_error = Some(e.to_string());
                                }
                            }
                        }
                    }
                });
                ui.separator();

                let modules_snapshot = { self.app.get_modules().clone() };
                ScrollArea::vertical().show(ui, |ui| {
                    let mut modules = modules_snapshot;
                    modules.sort_by(|a, b| {
                        let an = a
                            .get_base_dll_name()
                            .unwrap_or("Unknown")
                            .to_ascii_lowercase();
                        let bn = b
                            .get_base_dll_name()
                            .unwrap_or("Unknown")
                            .to_ascii_lowercase();
                        an.cmp(&bn)
                    });
                    let sigs_mut: &mut Vec<AppSignature> = unsafe { &mut *sigs_ptr };
                    for (idx, s) in sigs_mut.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("#{}", idx + 1));
                                let resp = ui.text_edit_singleline(&mut s.name);
                                if resp.changed() && s.name.chars().any(|c| c.is_whitespace()) {
                                    s.name.retain(|c| !c.is_whitespace());
                                }
                                if ui.button("Remove").clicked() {
                                    s.name = String::from("<removed>");
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Module:");
                                // Module dropdown
                                let mut current = s.module.clone();
                                egui::ComboBox::from_id_source(("sig_mod", idx))
                                    .selected_text(if current.is_empty() {
                                        "<select>".to_string()
                                    } else {
                                        current.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for m in &modules {
                                            let mname = m.get_base_dll_name().unwrap_or("Unknown");
                                            ui.selectable_value(
                                                &mut current,
                                                mname.to_string(),
                                                mname,
                                            );
                                        }
                                    });
                                if current != s.module {
                                    s.module = current;
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Pattern:");
                                ui.text_edit_singleline(&mut s.pattern);
                            });
                            if let Some(val) = s.last_value {
                                ui.label(format!("Resolved: 0x{:X}", val));
                            } else if let Some(err) = &s.last_error {
                                ui.colored_label(egui::Color32::RED, err.to_string());
                            }
                            ui.horizontal(|ui| {
                                ui.label("Offset:");
                                if s.offset_buf.is_empty() {
                                    s.offset_buf = format!("0x{:X}", s.offset);
                                }
                                let _ = ui.text_edit_singleline(&mut s.offset_buf);
                                ui.separator();
                                ui.checkbox(&mut s.is_relative, "Relative");
                                if s.is_relative {
                                    ui.label("InstLen:");
                                    if s.rel_inst_len_buf.is_empty() {
                                        s.rel_inst_len_buf = format!("{}", s.rel_inst_len);
                                    }
                                    let _ = ui.text_edit_singleline(&mut s.rel_inst_len_buf);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Copy resolved").clicked() {
                                    // Use cached value if available; otherwise resolve now
                                    let mut to_copy: Option<u64> = s.last_value;
                                    if to_copy.is_none() {
                                        if let Some(handle) = self.app.handle.as_ref() {
                                            let sig = if s.is_relative {
                                                handle::Signature::relative_address(
                                                    &s.name,
                                                    &s.pattern,
                                                    s.offset,
                                                    s.rel_inst_len,
                                                )
                                            } else {
                                                handle::Signature::offset(
                                                    &s.name, &s.pattern, s.offset,
                                                )
                                            };
                                            if let Ok(value) =
                                                handle.resolve_signature(&s.module, &sig)
                                            {
                                                s.last_value = Some(value);
                                                to_copy = Some(value);
                                            }
                                        }
                                    }
                                    if let Some(value) = to_copy {
                                        let _ = arboard::Clipboard::new().and_then(|mut cb| {
                                            cb.set_text(format!("0x{:X}", value))
                                        });
                                    }
                                }
                            });
                        });
                        ui.separator();
                    }
                    let sigs_mut: &mut Vec<AppSignature> = unsafe { &mut *sigs_ptr };
                    sigs_mut.retain(|s| s.name != "<removed>");
                });
            });
    }
}

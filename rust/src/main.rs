//! SCode (ex-StudentCode) : éditeur de code léger pour étudiants (version Rust).
#![allow(deprecated)]
#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod editor;
mod explorer;
mod fileicons_gen;
mod highlight;
mod icons;
mod imageview;
mod pdf;
mod pip;
mod secrets;
mod settings;
mod clip;
mod ssh;
mod vpn;
mod pyscripter;
mod runner;
mod terminal;
mod theme;

use eframe::egui;
use std::path::PathBuf;

fn icon() -> Option<egui::IconData> {
    let png = include_bytes!("../assets/icon-256.png");
    eframe::icon_data::from_png_bytes(png).ok()
}

/// Vérification rapide utilisée par la fabrication des versions (sans fenêtre).
fn selftest() -> i32 {
    use std::io::Write;
    let mut out = std::io::stdout();
    macro_rules! say { ($($t:tt)*) => { let _ = writeln!(out, $($t)*); } }
    let mut problems = Vec::new();
    let p = theme::palette("Tokyo Night");
    let job = highlight::highlight("int main(void) { return 0; }\n", highlight::Lang::C, &p, &egui::FontId::monospace(13.0));
    if job.text.len() != 29 {
        problems.push("coloration".to_string());
    }
    let tmp = std::env::temp_dir().join("studentcode-selftest.c");
    let _ = std::fs::write(&tmp, "int main(void){return 0;}\n");
    match runner::command_line(&tmp, &[], None) {
        Ok(_) => {}
        Err(e) if e.contains("introuvable") => { say!("note : {e}"); }
        Err(e) => problems.push(format!("commande C : {e}")),
    }
    say!("SCode {} · module PyScripter : {}", app::VERSION,
             pyscripter::module_dir().map(|d| d.display().to_string()).unwrap_or("absent".into()));
    say!("Python : {:?}", runner::python());
    match terminal::probe() {
        Ok(()) => { say!("terminal : OK"); }
        Err(e) => problems.push(format!("terminal : {e}")),
    }
    match pdf::module_lib() {
        Some(lib) => match pdf::check() {
            Ok(()) => { say!("module Lecteur PDF : {}", lib.display()); }
            Err(e) => problems.push(format!("module PDF : {e}")),
        },
        None => { say!("module Lecteur PDF : absent"); }
    }
    if problems.is_empty() {
        say!("SELFTEST OK");
        0
    } else {
        say!("ERREUR {}", problems.join(", "));
        1
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("SCode {}", app::VERSION);
        return Ok(());
    }
    if args.iter().any(|a| a == "--selftest") {
        std::process::exit(selftest());
    }
    let target = args.iter().skip(1).find(|a| !a.starts_with("--")).map(|a| {
        let p = PathBuf::from(a);
        if p.is_absolute() { p } else { std::env::current_dir().unwrap_or_default().join(p) }
    });
    let mut vp = egui::ViewportBuilder::default()
        .with_title("SCode")
        .with_app_id("studentcode")
        .with_inner_size(test_size().unwrap_or([1280.0, 800.0]))
        .with_min_inner_size([640.0, 400.0])
        .with_decorations(false)
        .with_transparent(false);
    if let Some(i) = icon() {
        vp = vp.with_icon(i);
    }
    let options = eframe::NativeOptions {
        viewport: vp,
        renderer: eframe::Renderer::Glow,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("StudentCode", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, target)))))
}

/// Taille de fenêtre imposée pour les captures de test (STUDENTCODE_SIZE=1400x820).
fn test_size() -> Option<[f32; 2]> {
    let v = std::env::var("STUDENTCODE_SIZE").ok()?;
    let (w, h) = v.split_once('x')?;
    Some([w.parse().ok()?, h.parse().ok()?])
}

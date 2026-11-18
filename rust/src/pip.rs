//! Bibliothèques Python (module PyScripter) : liste, installation, suppression avec pip.
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

pub const SUGGESTED: &[(&str, &str)] = &[
    ("numpy", "calcul numérique, tableaux"),
    ("matplotlib", "graphiques"),
    ("pandas", "tableaux de données"),
    ("scikit-learn", "apprentissage automatique (SVM…)"),
    ("scipy", "maths scientifiques"),
    ("pygame-ce", "jeux 2D"),
    ("requests", "requêtes web"),
    ("pillow", "images"),
    ("sympy", "calcul formel"),
    ("pytest", "tests"),
    ("flask", "petits sites web"),
    ("pycryptodome", "chiffrement"),
];

pub enum Msg {
    Line(String),
    Listed(Vec<(String, String)>),
    Done(bool),
}

fn pip_cmd(py: &[String], args: &[&str]) -> Command {
    let mut c = Command::new(&py[0]);
    c.args(&py[1..]).args(["-m", "pip", "--disable-pip-version-check"]).args(args);
    c.env("PYTHONIOENCODING", "utf-8");
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    c
}

pub fn list(py: Vec<String>, ctx: &eframe::egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let out = pip_cmd(&py, &["list", "--format=json"]).output();
        let pkgs = out.ok().and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| v.as_array().cloned()).unwrap_or_default().iter()
            .map(|p| (p["name"].as_str().unwrap_or("").to_string(), p["version"].as_str().unwrap_or("").to_string()))
            .collect::<Vec<_>>();
        let _ = tx.send(Msg::Listed(pkgs));
        ctx.request_repaint();
    });
    rx
}

/// install / uninstall ; sous Linux avec un Python du système, pip refuse sans --user
/// (et --break-system-packages sur Ubuntu récent) : on ajoute ce qu'il faut.
pub fn run(py: Vec<String>, install: bool, names: Vec<String>, ctx: &eframe::egui::Context) -> Receiver<Msg> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let mut args: Vec<String> = if install { vec!["install".into(), "--upgrade".into()] } else { vec!["uninstall".into(), "-y".into()] };
        let in_venv = std::env::var("VIRTUAL_ENV").is_ok();
        if install && cfg!(unix) && !in_venv {
            args.push("--user".into());
            args.push("--break-system-packages".into());
        }
        if !install && cfg!(unix) && !in_venv {
            args.push("--break-system-packages".into());
        }
        args.extend(names);
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let _ = tx.send(Msg::Line(format!("$ pip {}\n", refs.join(" "))));
        let child = pip_cmd(&py, &refs).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        let ok = match child {
            Ok(mut c) => {
                use std::io::{BufRead, BufReader};
                let err = c.stderr.take().unwrap();
                let tx2 = tx.clone();
                let ctx2 = ctx.clone();
                let h = std::thread::spawn(move || for l in BufReader::new(err).lines().map_while(Result::ok) {
                    let _ = tx2.send(Msg::Line(format!("{l}\n"))); ctx2.request_repaint(); });
                for l in BufReader::new(c.stdout.take().unwrap()).lines().map_while(Result::ok) {
                    let _ = tx.send(Msg::Line(format!("{l}\n")));
                    ctx.request_repaint();
                }
                let _ = h.join();
                c.wait().map(|s| s.success()).unwrap_or(false)
            }
            Err(e) => { let _ = tx.send(Msg::Line(format!("pip impossible : {e}\n"))); false }
        };
        let _ = tx.send(Msg::Done(ok));
        ctx.request_repaint();
    });
    rx
}

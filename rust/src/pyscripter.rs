//! Module PyScripter (facultatif, choisi à l'installation) : console Python ouverte sur les
//! variables du programme après son exécution, et débogueur pas à pas. Le travail côté Python
//! est fait par deux petits scripts fournis avec le module (run_backend.py, dbg_backend.py).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

pub const END_MARK: &str = "\x1e__LAZYPY_END__";

/// Dossier du module s'il est installé : à côté de l'exécutable (modules/pyscripter), ou dans
/// le dossier indiqué par STUDENTCODE_MODULES (développement).
pub fn module_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(d) = std::env::var("STUDENTCODE_MODULES") {
        candidates.push(PathBuf::from(d).join("pyscripter"));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..4 {
            let Some(d) = dir else { break };
            candidates.push(d.join("modules").join("pyscripter"));
            candidates.push(d.join("share").join("studentcode").join("modules").join("pyscripter"));
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }
    candidates.into_iter().find(|d| d.join("run_backend.py").is_file() && d.join("dbg_backend.py").is_file())
}

pub struct Frame {
    pub file: String,
    pub line: usize,
    pub func: String,
}

#[derive(Clone)]
pub struct Var {
    pub name: String,
    pub ty: String,
    pub repr: String,
    pub expr: String,
    pub expandable: bool,
}

pub enum DbgEvent {
    Connected,
    Stopped(Vec<Frame>),
    Vars { locals: Vec<Var>, globals: Vec<Var> },
    Children { id: u64, items: Vec<Var> },
    Eval { text: String, error: bool },
    Exception { file: Option<String>, line: Option<usize>, text: String },
    Closed,
}

pub struct Session {
    stream: std::sync::Arc<std::sync::Mutex<Option<TcpStream>>>,
    pub rx: Receiver<DbgEvent>,
    next_id: u64,
}

fn vars(v: &Value) -> Vec<Var> {
    v.as_array().map(|a| a.iter().map(|x| Var {
        name: x["name"].as_str().unwrap_or("").into(),
        ty: x["type"].as_str().unwrap_or("").into(),
        repr: x["repr"].as_str().unwrap_or("").into(),
        expr: x["expr"].as_str().unwrap_or("").into(),
        expandable: x["exp"].as_bool().unwrap_or(false),
    }).collect()).unwrap_or_default()
}

impl Session {
    /// Ouvre le port d'écoute ; renvoie la session et le port à donner à dbg_backend.py.
    pub fn listen(breaks: Vec<(String, usize)>, ctx: eframe::egui::Context) -> std::io::Result<(Session, u16)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let (tx, rx) = channel();
        let shared = std::sync::Arc::new(std::sync::Mutex::new(None::<TcpStream>));
        let writer = shared.clone();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut w = match stream.try_clone() { Ok(w) => w, Err(_) => return };
            let init = json!({"breaks": breaks.iter().map(|(f, l)| json!([f, l])).collect::<Vec<_>>()});
            let _ = w.write_all(format!("{init}\n").as_bytes());
            *writer.lock().unwrap() = Some(w);
            let _ = tx.send(DbgEvent::Connected);
            ctx.request_repaint();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let Ok(m) = serde_json::from_str::<Value>(&line) else { continue };
                let ev = match m["event"].as_str().unwrap_or("") {
                    "stopped" => DbgEvent::Stopped(m["stack"].as_array().map(|a| a.iter().map(|f| Frame {
                        file: f["file"].as_str().unwrap_or("").into(),
                        line: f["line"].as_u64().unwrap_or(1) as usize,
                        func: f["func"].as_str().unwrap_or("").into(),
                    }).collect()).unwrap_or_default()),
                    "vars" => DbgEvent::Vars { locals: vars(&m["locals"]), globals: vars(&m["globals"]) },
                    "children" => DbgEvent::Children { id: m["id"].as_u64().unwrap_or(0), items: vars(&m["items"]) },
                    "eval" => DbgEvent::Eval { text: m["text"].as_str().unwrap_or("").into(), error: m["error"].as_bool().unwrap_or(false) },
                    "exception" => DbgEvent::Exception {
                        file: m["file"].as_str().map(String::from),
                        line: m["line"].as_u64().map(|x| x as usize),
                        text: m["text"].as_str().unwrap_or("").into(),
                    },
                    _ => continue,
                };
                let _ = tx.send(ev);
                ctx.request_repaint();
            }
            let _ = tx.send(DbgEvent::Closed);
            ctx.request_repaint();
        });
        Ok((Session { stream: shared, rx, next_id: 1 }, port))
    }

    pub fn send(&mut self, v: Value) {
        if let Some(s) = self.stream.lock().unwrap().as_mut() {
            let _ = s.write_all(format!("{v}\n").as_bytes());
        }
    }

    pub fn cmd(&mut self, c: &str) {
        self.send(json!({"cmd": c}));
    }

    pub fn request_vars(&mut self, frame: usize) {
        self.send(json!({"cmd": "vars", "frame": frame}));
    }

    pub fn request_children(&mut self, frame: usize, expr: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"cmd": "children", "frame": frame, "expr": expr, "id": id}));
        id
    }

    pub fn eval(&mut self, frame: usize, expr: &str) {
        self.send(json!({"cmd": "eval", "frame": frame, "expr": expr}));
    }

    pub fn set_break(&mut self, file: &Path, line: usize, on: bool) {
        self.send(json!({"cmd": "break", "file": file.to_string_lossy(), "line": line, "on": on}));
    }
}

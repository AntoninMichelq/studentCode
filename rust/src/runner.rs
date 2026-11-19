//! Exécution des programmes : C/C++ compilés puis lancés, Java compilé (tous les .java du dossier),
//! scripts confiés à leur interpréteur. La sortie arrive par un canal, la saisie part sur stdin.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

pub enum Out {
    Stdout(String),
    Stderr(String),
}

/// Un programme lancé (sur cet ordinateur ou sur une machine distante).
pub trait Proc {
    fn try_recv(&mut self) -> Option<Out>;
    fn write(&mut self, text: &str);
    fn kill(&mut self);
    fn poll_exit(&mut self) -> Option<Option<i32>>;
    fn finished(&self) -> bool;
}

pub struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    pub rx: Receiver<Out>,
    pub finished: bool,
}

/// Petit fichier C compilé avec le programme : la sortie s'affiche tout de suite (sinon les
/// printf n'arrivent qu'à la fin quand la sortie n'est pas un terminal).
const NOBUF_C: &str = "#include <stdio.h>\n__attribute__((constructor)) static void studentcode_nobuf(void) {\n    \
    setvbuf(stdout, NULL, _IONBF, 0);\n    setvbuf(stderr, NULL, _IONBF, 0);\n}\n";

pub fn which(cmd: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or(".EXE;.CMD;.BAT".into()).split(';').map(|s| s.to_lowercase()).collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        for e in &exts {
            let p = dir.join(format!("{cmd}{e}"));
            if p.is_file() {
                if cfg!(windows) && p.to_string_lossy().contains("WindowsApps") && cmd.starts_with("python") {
                    continue; // faux python de Windows qui ouvre le Microsoft Store
                }
                return Some(p);
            }
        }
    }
    None
}

/// Commande qui lance Python sur cet ordinateur.
pub fn python() -> Option<Vec<String>> {
    if cfg!(windows) {
        if which("py").is_some() {
            return Some(vec!["py".into(), "-3".into()]);
        }
        if which("python").is_some() {
            return Some(vec!["python".into()]);
        }
        None
    } else {
        which("python3").map(|_| vec!["python3".into()]).or_else(|| which("python").map(|_| vec!["python".into()]))
    }
}

pub const PYTHON_MISSING: &str = "Python est introuvable. Installe-le (python.org, en cochant « Add python.exe to PATH » \
    sous Windows, ou sudo apt install python3 sous Linux), puis relance StudentCode.";

fn build_dir(folder: &Path) -> PathBuf {
    let mut h: u64 = 1469598103934665603;
    for b in folder.to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    std::env::temp_dir().join("studentcode-build").join(format!("{:010x}", h & 0xff_ffff_ffff))
}

fn java_package(path: &Path) -> String {
    let src = std::fs::read_to_string(path).unwrap_or_default();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("package ") {
            return format!("{}.", rest.trim_end_matches(';').trim());
        }
        if t.starts_with("import ") || t.starts_with("public ") || t.starts_with("class ") {
            break;
        }
    }
    String::new()
}

#[cfg(not(windows))]
fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
#[cfg(windows)]
fn q(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '\t', '&', '(', ')', '^', ',', ';']) { format!("\"{s}\"") } else { s.to_string() }
}

/// Ligne de commande (bash sous Linux, cmd sous Windows) qui exécute `path`, ou un message
/// qui explique ce qui manque.
pub fn command_line(path: &Path, args: &[String], python_cmd: Option<&[String]>) -> Result<String, String> {
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let build = build_dir(&folder);
    let extra: String = args.iter().map(|a| format!(" {}", q(a))).collect();
    let need = |tool: &str, help: &str| -> Result<(), String> {
        if which(tool).is_none() { Err(format!("« {tool} » est introuvable. {help}")) } else { Ok(()) }
    };
    let cd = if cfg!(windows) { format!("cd /d {}", q(&folder.to_string_lossy())) } else { format!("cd {}", q(&folder.to_string_lossy())) };
    let gcc_help = if cfg!(windows) {
        "Installe MinGW-w64 (par exemple avec MSYS2 : https://www.msys2.org) et ajoute son dossier bin au PATH."
    } else {
        "Installe-le avec : sudo apt install build-essential"
    };
    let java_help = if cfg!(windows) { "Installe un JDK, par exemple Temurin : https://adoptium.net" } else {
        "Installe-le avec : sudo apt install default-jdk" };
    match ext.as_str() {
        "c" | "cpp" | "cc" | "cxx" => {
            let cc = if ext == "c" { "gcc" } else { "g++" };
            need(cc, gcc_help)?;
            std::fs::create_dir_all(&build).map_err(|e| e.to_string())?;
            let helper = build.join("studentcode_nobuf.c");
            let _ = std::fs::write(&helper, NOBUF_C);
            let exe = build.join(if cfg!(windows) { format!("{stem}.exe") } else { stem.clone() });
            let (exe_s, helper_s) = (exe.to_string_lossy().to_string(), helper.to_string_lossy().to_string());
            // les bibliothèques à lier : raylib, maths… (projets simples sans Makefile)
            let src = std::fs::read_to_string(path).unwrap_or_default();
            let mut libs = String::from(" -lm");
            if src.contains("raylib.h") {
                libs = if cfg!(windows) { " -lraylib -lopengl32 -lgdi32 -lwinmm".into() } else { " -lraylib -lGL -lm -lpthread -ldl -lrt -lX11".into() };
            }
            if cfg!(windows) && !src.contains("raylib.h") { libs.clear(); }
            Ok(format!("{cd} && {cc} -Wall -g {} {} -o {}{libs} && {}{extra}", q(&name), q(&helper_s), q(&exe_s), q(&exe_s)))
        }
        "java" => {
            need("javac", java_help)?;
            std::fs::create_dir_all(&build).map_err(|e| e.to_string())?;
            let b = q(&build.to_string_lossy());
            Ok(format!("{cd} && javac -d {b} *.java && java -cp {b} {}{stem}{extra}", java_package(path)))
        }
        "py" | "pyw" => {
            let py = python_cmd.ok_or(PYTHON_MISSING.to_string())?;
            Ok(format!("{cd} && {} -u {}{extra}", py.join(" "), q(&name)))
        }
        "js" | "mjs" => { need("node", "Installe Node.js : https://nodejs.org")?; Ok(format!("{cd} && node {}{extra}", q(&name))) }
        "rs" => { need("rustc", "Installe Rust : https://rustup.rs")?;
            let exe = build.join(&stem); std::fs::create_dir_all(&build).map_err(|e| e.to_string())?;
            let e = q(&exe.to_string_lossy());
            Ok(format!("{cd} && rustc -g {} -o {e} && {e}{extra}", q(&name))) }
        "go" => { need("go", "Installe Go : https://go.dev")?; Ok(format!("{cd} && go run {}{extra}", q(&name))) }
        "sh" | "bash" => { need("bash", "")?; Ok(format!("{cd} && bash {}{extra}", q(&name))) }
        "pl" => Ok(format!("{cd} && perl {}{extra}", q(&name))),
        "rb" => Ok(format!("{cd} && ruby {}{extra}", q(&name))),
        "lua" => Ok(format!("{cd} && lua {}{extra}", q(&name))),
        "php" => Ok(format!("{cd} && php {}{extra}", q(&name))),
        "bat" | "cmd" if cfg!(windows) => Ok(format!("{cd} && call {}{extra}", q(&name))),
        "ps1" => Ok(format!("{cd} && powershell -NoProfile -ExecutionPolicy Bypass -File {}{extra}", q(&name))),
        _ if name.eq_ignore_ascii_case("makefile") => { need("make", gcc_help)?; Ok(format!("{cd} && make")) }
        _ => Err("Je ne sais pas exécuter ce type de fichier.".into()),
    }
}

/// Commande système qui exécute une ligne de shell.
pub fn shell(line: &str, cwd: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new(std::env::var("COMSPEC").unwrap_or("cmd.exe".into()));
        c.raw_arg(format!("/d /s /c \"{line}\""));
        c.creation_flags(0x0800_0000); // pas de fenêtre de console
        c.current_dir(cwd);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("bash");
        c.arg("-c").arg(line).current_dir(cwd);
        c
    }
}

pub fn spawn(mut cmd: Command) -> std::io::Result<Running> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.env("PYTHONUNBUFFERED", "1").env("PYTHONIOENCODING", "utf-8");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // pour pouvoir arrêter le programme et tout ce qu'il a lancé
    }
    let mut child = cmd.spawn()?;
    let (tx, rx) = channel();
    pump(child.stdout.take().unwrap(), tx.clone(), false);
    pump(child.stderr.take().unwrap(), tx.clone(), true);
    let stdin = child.stdin.take();
    Ok(Running { child, stdin, rx, finished: false })
}

fn pump(mut r: impl Read + Send + 'static, tx: Sender<Out>, err: bool) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    // ne pas couper un caractère UTF-8 en deux
                    let cut = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(e) if e.error_len().is_none() => e.valid_up_to(),
                        Err(_) => pending.len(),
                    };
                    let text = String::from_utf8_lossy(&pending[..cut]).to_string();
                    pending.drain(..cut);
                    let _ = tx.send(if err { Out::Stderr(text) } else { Out::Stdout(text) });
                }
            }
        }
    });
}

impl Proc for Running {
    fn try_recv(&mut self) -> Option<Out> {
        self.rx.try_recv().ok()
    }
    fn write(&mut self, text: &str) {
        Running::write(self, text)
    }
    fn kill(&mut self) {
        Running::kill(self)
    }
    fn poll_exit(&mut self) -> Option<Option<i32>> {
        Running::poll_exit(self)
    }
    fn finished(&self) -> bool {
        self.finished
    }
}

impl Running {
    pub fn write(&mut self, text: &str) {
        if let Some(s) = self.stdin.as_mut() {
            let _ = s.write_all(text.as_bytes());
            let _ = s.flush();
        }
    }

    /// Code de sortie si le programme est terminé.
    pub fn poll_exit(&mut self) -> Option<Option<i32>> {
        if self.finished {
            return None;
        }
        match self.child.try_wait() {
            Ok(Some(st)) => {
                self.finished = true;
                Some(st.code())
            }
            _ => None,
        }
    }

    pub fn kill(&mut self) {
        let pid = self.child.id();
        #[cfg(unix)]
        {
            let _ = Command::new("kill").arg("-KILL").arg(format!("-{pid}")).status();
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &pid.to_string()])
                .creation_flags(0x0800_0000).status();
        }
        let _ = self.child.kill();
    }
}

/// Retire les séquences de couleur ANSI d'un texte de sortie.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            if it.peek() == Some(&'[') {
                it.next();
                while let Some(&n) = it.peek() {
                    it.next();
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Commande bash qui exécute un fichier sur une machine Linux distante (SSH).
pub fn remote_command(path: &str, args: &[String]) -> Option<String> {
    let (folder, name) = path.rsplit_once('/').map(|(a, b)| (if a.is_empty() { "/" } else { a }, b)).unwrap_or((".", path));
    let (stem, ext) = name.rsplit_once('.').map(|(s, e)| (s, e.to_lowercase())).unwrap_or((name, String::new()));
    let extra: String = args.iter().map(|a| format!(" {}", sq(a))).collect();
    let out = format!("/tmp/studentcode-build-$(id -u)/{}", folder.replace('/', "_"));
    // affichage immédiat des printf si stdbuf existe sur la machine
    let nobuf = "$(command -v stdbuf >/dev/null 2>&1 && echo stdbuf -o0 -e0) ";
    let cd = format!("cd {}", sq(folder));
    let line = match ext.as_str() {
        "py" => format!("{cd} && exec python3 -u {}{extra}", sq(name)),
        "sh" | "bash" => format!("{cd} && exec bash {}{extra}", sq(name)),
        "pl" => format!("{cd} && exec perl {}{extra}", sq(name)),
        "rb" => format!("{cd} && exec ruby {}{extra}", sq(name)),
        "js" => format!("{cd} && exec node {}{extra}", sq(name)),
        "c" | "cpp" | "cc" | "cxx" => {
            let cc = if ext == "c" { "gcc" } else { "g++" };
            format!("{cd} && mkdir -p \"{out}\" && {cc} -Wall -g {} -o \"{out}/{stem}\" -lm && exec {nobuf}\"{out}/{stem}\"{extra}", sq(name))
        }
        "java" => format!("{cd} && mkdir -p \"{out}\" && javac -d \"{out}\" *.java && \
            pkg=$(sed -n 's/^[[:space:]]*package[[:space:]]\\+\\([A-Za-z0-9_.]*\\)[[:space:]]*;.*/\\1./p' {} | head -1) && \
            exec java -cp \"{out}\" \"${{pkg}}{stem}\"{extra}", sq(name)),
        _ if name.eq_ignore_ascii_case("makefile") => format!("{cd} && make"),
        _ => return None,
    };
    Some(line)
}

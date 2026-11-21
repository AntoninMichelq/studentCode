//! VPN : sous Linux par NetworkManager (nmcli, comme les réglages réseau d'Ubuntu), sous
//! Windows par OpenVPN GUI. Option « garder internet » : seul le réseau du lab passe par le VPN.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Debug)]
pub struct Vpn {
    pub name: String,
    pub id: String, // uuid NetworkManager, ou nom sous Windows
    pub active: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Opts {
    pub split: bool,
    pub routes: String,
    pub hosts: Vec<String>,
    pub user: String,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { split: true, routes: String::new(), hosts: Vec::new(), user: String::new() }
    }
}

fn opts_file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().join("vpn.json"))
}

pub fn all_opts() -> BTreeMap<String, Opts> {
    opts_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn opts(id: &str) -> Opts {
    all_opts().get(id).cloned().unwrap_or_default()
}

pub fn save_opts(id: &str, o: &Opts) {
    let mut m = all_opts();
    m.insert(id.to_string(), o.clone());
    if let Some(f) = opts_file() {
        if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
        if let Ok(j) = serde_json::to_string_pretty(&m) { let _ = std::fs::write(f, j); }
    }
}

/// « 10.22.0.0/16, 10.0.3.7 » → réseaux IPv4 valides (« a.b.c.d/n »).
pub fn clean_routes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()) {
        let (ip, len) = part.split_once('/').map(|(a, b)| (a, b.parse::<u8>().ok())).unwrap_or((part, Some(32)));
        let (Ok(addr), Some(len)) = (ip.parse::<std::net::Ipv4Addr>(), len) else { continue };
        if len > 32 { continue; }
        let mask: u32 = if len == 0 { 0 } else { u32::MAX << (32 - len) };
        let net = std::net::Ipv4Addr::from(u32::from(addr) & mask);
        let s = format!("{net}/{len}");
        if !out.contains(&s) { out.push(s); }
    }
    out
}

#[allow(dead_code)]
fn netmask(len: u8) -> String {
    let m: u32 = if len == 0 { 0 } else { u32::MAX << (32 - len) };
    std::net::Ipv4Addr::from(m).to_string()
}

pub fn nm_error(text: &str) -> String {
    let low = text.to_lowercase();
    if low.contains("plugin") && (low.contains("not") || low.contains("missing") || low.contains("unknown")) {
        return "Le module OpenVPN de NetworkManager manque : sudo apt install network-manager-openvpn-gnome".into();
    }
    if low.contains("secrets were required") || low.contains("no valid secrets") || low.contains("login failed") {
        return "Identifiant ou mot de passe du VPN refusé.".into();
    }
    if low.contains("timeout") || low.contains("timed out") {
        return "Le serveur VPN ne répond pas (connexion internet ? serveur accessible d'ici ?).".into();
    }
    if low.contains("not authorized") || low.contains("insufficient privileges") {
        return "Ton compte n'a pas le droit de gérer les connexions réseau.".into();
    }
    let t = text.trim();
    if t.is_empty() { "erreur inconnue".into() } else { t.chars().take(400).collect() }
}

pub struct Job {
    pub rx: Receiver<Result<String, String>>,
    pub what: String,
}

fn run(mut c: Command) -> Result<String, String> {
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    let o = c.output().map_err(|e| format!("commande impossible : {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr);
    if o.status.success() { Ok(out) } else { Err(out) }
}

fn job(what: &str, ctx: &eframe::egui::Context, f: impl FnOnce() -> Result<String, String> + Send + 'static) -> Job {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(f());
        ctx.request_repaint();
    });
    Job { rx, what: what.into() }
}

// ======================================================================= Linux (nmcli)
#[cfg(not(windows))]
pub fn available() -> Result<(), String> {
    crate::runner::which("nmcli").map(|_| ()).ok_or("NetworkManager (nmcli) est introuvable : le VPN ne peut pas être géré d'ici.".into())
}

#[cfg(not(windows))]
fn split_nm(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in line.chars() {
        if esc { out.last_mut().unwrap().push(c); esc = false; }
        else if c == '\\' { esc = true; }
        else if c == ':' { out.push(String::new()); }
        else { out.last_mut().unwrap().push(c); }
    }
    out
}

#[cfg(not(windows))]
pub fn list() -> Result<Vec<Vpn>, String> {
    let out = run({ let mut c = Command::new("nmcli"); c.args(["-t", "-f", "NAME,UUID,TYPE,ACTIVE", "connection", "show"]); c })?;
    Ok(out.lines().filter_map(|l| {
        let f = split_nm(l);
        if f.len() < 4 || !(f[2] == "vpn" || f[2] == "wireguard") { return None; }
        Some(Vpn { name: f[0].clone(), id: f[1].clone(), active: f[3] == "yes" })
    }).collect())
}

#[cfg(not(windows))]
fn route_args(id: &str) -> Vec<String> {
    let o = opts(id);
    if !o.split {
        return ["ipv4.never-default", "no", "ipv6.never-default", "no", "ipv4.routes", ""].map(String::from).to_vec();
    }
    let routes = clean_routes(&format!("{} {}", o.routes, o.hosts.join(" ")));
    vec!["ipv4.never-default".into(), "yes".into(), "ipv6.never-default".into(), "yes".into(),
         "ipv4.routes".into(), routes.join(", ")]
}

#[cfg(not(windows))]
pub fn import(path: &Path, ctx: &eframe::egui::Context) -> Job {
    let path = path.to_path_buf();
    job("import", ctx, move || {
        let out = run({ let mut c = Command::new("nmcli"); c.args(["connection", "import", "type", "openvpn", "file"]).arg(&path); c })
            .map_err(|e| nm_error(&e))?;
        // « Connexion « x » (uuid) ajoutée avec succès »
        if let (Some(a), Some(b)) = (out.find('('), out.find(')')) {
            let id = out[a + 1..b].to_string();
            let mut args = vec!["connection".to_string(), "modify".into(), "uuid".into(), id.clone()];
            args.extend(route_args(&id));
            let _ = run({ let mut c = Command::new("nmcli"); c.args(&args); c });
        }
        Ok("VPN ajouté.".into())
    })
}

#[cfg(not(windows))]
pub fn connect(v: &Vpn, user: &str, password: &str, ctx: &eframe::egui::Context) -> Job {
    let (id, user, password) = (v.id.clone(), user.to_string(), password.to_string());
    job("up", ctx, move || {
        if !user.is_empty() {
            let _ = run({ let mut c = Command::new("nmcli"); c.args(["connection", "modify", "uuid", &id, "+vpn.data", &format!("username={user}")]); c });
        }
        let mut args = vec!["connection".to_string(), "modify".into(), "uuid".into(), id.clone()];
        args.extend(route_args(&id));
        let _ = run({ let mut c = Command::new("nmcli"); c.args(&args); c });
        let mut c = Command::new("nmcli");
        c.args(["--wait", "45", "connection", "up", "uuid", &id]);
        let mut tmp = None;
        if !password.is_empty() {
            let p = std::env::temp_dir().join(format!("sc-vpn-{}", std::process::id()));
            {
                use std::os::unix::fs::OpenOptionsExt;
                let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&p)
                    .map_err(|e| e.to_string())?;
                use std::io::Write;
                let _ = writeln!(f, "vpn.secrets.password:{password}");
            }
            c.arg("passwd-file").arg(&p);
            tmp = Some(p);
        }
        let r = run(c);
        if let Some(p) = tmp { let _ = std::fs::remove_file(p); }
        r.map(|_| "VPN connecté.".to_string()).map_err(|e| nm_error(&e))
    })
}

#[cfg(not(windows))]
pub fn disconnect(v: &Vpn, ctx: &eframe::egui::Context) -> Job {
    let id = v.id.clone();
    job("down", ctx, move || run({ let mut c = Command::new("nmcli"); c.args(["connection", "down", "uuid", &id]); c })
        .map(|_| "VPN déconnecté.".into()).map_err(|e| nm_error(&e)))
}

#[cfg(not(windows))]
pub fn delete(v: &Vpn, ctx: &eframe::egui::Context) -> Job {
    let id = v.id.clone();
    job("delete", ctx, move || run({ let mut c = Command::new("nmcli"); c.args(["connection", "delete", "uuid", &id]); c })
        .map(|_| "VPN supprimé.".into()).map_err(|e| nm_error(&e)))
}

// ======================================================================= Windows (OpenVPN GUI)
#[cfg(windows)]
fn gui() -> Option<PathBuf> {
    let mut c = vec![];
    if let Some(p) = crate::runner::which("openvpn-gui") { c.push(p); }
    for base in [std::env::var("ProgramFiles").ok(), std::env::var("ProgramFiles(x86)").ok()].into_iter().flatten() {
        c.push(PathBuf::from(base).join("OpenVPN").join("bin").join("openvpn-gui.exe"));
    }
    c.into_iter().find(|p| p.is_file())
}

#[cfg(windows)]
fn conf_dir() -> PathBuf {
    directories::BaseDirs::new().map(|b| b.home_dir().join("OpenVPN").join("config")).unwrap_or_default()
}

#[cfg(windows)]
pub fn available() -> Result<(), String> {
    gui().map(|_| ()).ok_or("OpenVPN GUI n'est pas installé : installe « OpenVPN Community » \
        (https://openvpn.net/community-downloads/), puis relance SCode.".into())
}

#[cfg(windows)]
fn last_file() -> PathBuf { conf_dir().join(".studentcode-last") }

#[cfg(windows)]
pub fn list() -> Result<Vec<Vpn>, String> {
    let active = run({ let mut c = Command::new("powershell"); c.args(["-NoProfile", "-Command",
        "(Get-NetAdapter | Where-Object { $_.InterfaceDescription -match 'TAP-Windows|OpenVPN|Wintun' -and $_.Status -eq 'Up' } | Measure-Object).Count"]); c })
        .ok().and_then(|o| o.trim().parse::<u32>().ok()).unwrap_or(0) > 0;
    let last = std::fs::read_to_string(last_file()).unwrap_or_default();
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(conf_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().join(format!("{name}.ovpn")).is_file() {
                v.push(Vpn { active: active && last.trim() == name, id: name.clone(), name });
            }
        }
    }
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(v)
}

#[cfg(windows)]
fn write_config(name: &str) {
    let folder = conf_dir().join(name);
    let Ok(src) = std::fs::read_to_string(folder.join(format!("{name}.source"))) else { return };
    let mut text = src.trim_end().to_string() + "\n";
    let o = opts(name);
    if o.split {
        text += "\n# --- ajouté par StudentCode : internet ne passe pas par le VPN ---\npull-filter ignore \"redirect-gateway\"\n";
        for r in clean_routes(&format!("{} {}", o.routes, o.hosts.join(" "))) {
            let (ip, len) = r.split_once('/').unwrap();
            text += &format!("route {ip} {}\n", netmask(len.parse().unwrap_or(32)));
        }
    }
    let _ = std::fs::write(folder.join(format!("{name}.ovpn")), text);
}

#[cfg(windows)]
pub fn import(path: &Path, ctx: &eframe::egui::Context) -> Job {
    let path = path.to_path_buf();
    job("import", ctx, move || {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("vpn".into());
        let name: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
        let folder = conf_dir().join(&name);
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        std::fs::copy(&path, folder.join(format!("{name}.source"))).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if [".crt", ".key", ".pem", ".p12", ".txt"].iter().any(|x| n.ends_with(x)) {
                    let _ = std::fs::copy(e.path(), folder.join(e.file_name()));
                }
            }
        }
        write_config(&name);
        Ok("VPN ajouté.".into())
    })
}

#[cfg(windows)]
pub fn connect(v: &Vpn, _user: &str, _password: &str, ctx: &eframe::egui::Context) -> Job {
    let name = v.id.clone();
    job("up", ctx, move || {
        let g = gui().ok_or("OpenVPN GUI n'est pas installé")?;
        write_config(&name);
        let _ = std::fs::write(last_file(), &name);
        let running = run({ let mut c = Command::new("tasklist"); c.args(["/FI", "IMAGENAME eq openvpn-gui.exe"]); c })
            .map(|o| o.to_lowercase().contains("openvpn-gui")).unwrap_or(false);
        let mut c = Command::new(&g);
        if running { c.args(["--command", "connect", &name]); } else { c.args(["--connect", &format!("{name}.ovpn")]); }
        c.spawn().map_err(|e| e.to_string())?;
        // OpenVPN demande l'identifiant dans sa propre fenêtre : on attend que l'adaptateur monte
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if list().map(|l| l.iter().any(|x| x.id == name && x.active)).unwrap_or(false) {
                return Ok("VPN connecté.".into());
            }
        }
        Err("le VPN ne s'est pas connecté (regarde la fenêtre d'OpenVPN)".into())
    })
}

#[cfg(windows)]
pub fn disconnect(v: &Vpn, ctx: &eframe::egui::Context) -> Job {
    let name = v.id.clone();
    job("down", ctx, move || {
        let g = gui().ok_or("OpenVPN GUI n'est pas installé")?;
        let _ = Command::new(g).args(["--command", "disconnect", &name]).spawn();
        std::thread::sleep(std::time::Duration::from_secs(3));
        Ok("VPN déconnecté.".into())
    })
}

#[cfg(windows)]
pub fn delete(v: &Vpn, ctx: &eframe::egui::Context) -> Job {
    let name = v.id.clone();
    job("delete", ctx, move || {
        std::fs::remove_dir_all(conf_dir().join(&name)).map_err(|e| e.to_string())?;
        Ok("VPN supprimé.".into())
    })
}

pub fn list_job(ctx: &eframe::egui::Context) -> Receiver<Result<Vec<Vpn>, String>> {
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    std::thread::spawn(move || { let _ = tx.send(list()); ctx.request_repaint(); });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn routes() {
        assert_eq!(super::clean_routes("10.11.0.15/28, 10.0.3.7; x 300.1.1.1"), vec!["10.11.0.0/28", "10.0.3.7/32"]);
        assert_eq!(super::netmask(28), "255.255.255.240");
    }
}

#[cfg(all(test, unix))]
mod fake_nm {
    #[test]
    #[ignore]
    fn nmcli_flows() {
        let ctx = eframe::egui::Context::default();
        let l = super::list().unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "lab:remote");
        assert!(!l[0].active && l[1].active);
        let j = super::import(std::path::Path::new("/tmp/x.ovpn"), &ctx);
        assert_eq!(j.rx.recv().unwrap().unwrap(), "VPN ajouté.");
        let mut o = super::Opts::default();
        o.routes = "10.11.0.0/28".into();
        o.hosts = vec!["10.11.0.2".into()];
        super::save_opts("bbbb-2", &o);
        let j = super::connect(&l[0], "etu", "mauvais", &ctx);
        assert!(j.rx.recv().unwrap().unwrap_err().contains("refusé"));
        let j = super::connect(&l[0], "etu", "bon", &ctx);
        assert_eq!(j.rx.recv().unwrap().unwrap(), "VPN connecté.");
        let log = std::fs::read_to_string("/tmp/fakenm/log").unwrap();
        assert!(log.contains("ipv4.routes 10.11.0.0/28, 10.11.0.2/32"), "{log}");
        assert!(log.contains("+vpn.data username=etu"));
        assert!(log.contains("ipv4.never-default yes"));
        // le fichier du mot de passe est supprimé
        assert!(!std::path::Path::new(&std::env::temp_dir().join(format!("sc-vpn-{}", std::process::id()))).exists());
    }
}

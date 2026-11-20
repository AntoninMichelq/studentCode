//! SSH : labs (machines enregistrées), connexion, fichiers distants (SFTP), exécution distante.
//! Chaque usage (explorateur, terminal, exécution) a sa propre connexion : aucune ne bloque
//! les autres.
use crate::runner::{Out, Proc};
use serde::{Deserialize, Serialize};
use ssh2::Session;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::Duration;

pub const CLEANUP: &str = include_str!("cleanup.sh");
pub const ENABLE_SSH: &str = "sudo apt install -y openssh-server && sudo systemctl enable --now ssh";
pub const ENABLE_PASSWORD: &str = "sudo sed -i 's/^#\\?PasswordAuthentication.*/PasswordAuthentication yes/' \
    /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf && sudo systemctl restart ssh";

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Lab {
    pub name: String,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub vpn: String,
    pub cleanup: bool,
    pub save_password: bool,
}

impl Default for Lab {
    fn default() -> Lab {
        Lab { name: String::new(), user: "ubuntu".into(), host: String::new(), port: 22, vpn: String::new(),
              cleanup: true, save_password: true }
    }
}

impl Lab {
    pub fn label(&self) -> String {
        if self.name.trim().is_empty() { format!("{}@{}", self.user, self.host) } else { self.name.clone() }
    }
    pub fn key(&self) -> String {
        format!("ssh-{}@{}:{}", self.user, self.host, self.port)
    }
}

/// Ouvre le terminal du système avec « ssh » vers la machine (menu « … » du panneau SSH).
pub fn external_terminal(lab: &Lab) {
    let target = format!("{}@{}", lab.user, lab.host);
    let port = lab.port.to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd").args(["/c", "start", "", "cmd", "/k", "ssh", "-p", &port, &target])
            .creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    {
        for (term, sep) in [("x-terminal-emulator", "-e"), ("gnome-terminal", "--"), ("konsole", "-e"), ("xfce4-terminal", "-x"), ("xterm", "-e")] {
            if crate::runner::which(term).is_some() {
                let _ = std::process::Command::new(term).args([sep, "ssh", "-p", &port, &target]).spawn();
                return;
            }
        }
    }
}

/// « ubuntu@10.11.0.2 », « Serveur = ubuntu@10.11.0.2 », « 10.11.0.2:2222 »
pub fn quick_parse(text: &str) -> Option<Lab> {
    let (name, rest) = match text.split_once('=') {
        Some((n, r)) => (n.trim().to_string(), r.trim()),
        None => (String::new(), text.trim()),
    };
    let rest = rest.trim_start_matches("ssh ").trim();
    if rest.is_empty() || rest.contains(' ') {
        return None;
    }
    let (user, hostport) = match rest.split_once('@') {
        Some((u, h)) => (u.to_string(), h),
        None => ("ubuntu".to_string(), rest),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap()),
        _ => (hostport.to_string(), 22),
    };
    if host.is_empty() || user.is_empty() {
        return None;
    }
    Some(Lab { name, user, host, port, ..Default::default() })
}

fn labs_file() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "StudentCode").map(|d| d.config_dir().join("labs.json"))
}

pub fn load_labs() -> Vec<Lab> {
    labs_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_labs(labs: &[Lab]) {
    if let Some(f) = labs_file() {
        if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
        if let Ok(j) = serde_json::to_string_pretty(labs) { let _ = std::fs::write(f, j); }
    }
}

/// Ouvre une session authentifiée.
pub fn connect(lab: &Lab, password: &str) -> Result<Session, String> {
    let addr = (lab.host.as_str(), lab.port).to_socket_addrs().map_err(|e| format!("adresse invalide : {e}"))?
        .next().ok_or("adresse introuvable")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(8)).map_err(|e| {
        match e.kind() {
            std::io::ErrorKind::ConnectionRefused => "connexion refusée (SSH n'écoute pas sur cette machine)".to_string(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => "la machine ne répond pas".to_string(),
            _ => e.to_string(),
        }
    })?;
    let _ = tcp.set_nodelay(true);
    let mut s = Session::new().map_err(|e| e.to_string())?;
    s.set_tcp_stream(tcp);
    s.set_timeout(20_000);
    s.handshake().map_err(|e| format!("échec de la poignée de main SSH : {}", e.message()))?;
    s.set_keepalive(true, 30);
    let methods = s.auth_methods(&lab.user).unwrap_or("password").to_string();
    if !password.is_empty() {
        if s.userauth_password(&lab.user, password).is_err() {
            if !methods.contains("password") {
                return Err(format!("AUTHMETHOD:{methods}"));
            }
            return Err("AUTH".into());
        }
    } else {
        // sans mot de passe : clé SSH de l'utilisateur ou agent
        let _ = s.userauth_agent(&lab.user);
        if !s.authenticated() {
            if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().join(".ssh")) {
                for k in ["id_ed25519", "id_rsa", "id_ecdsa"] {
                    let p = home.join(k);
                    if p.is_file() && s.userauth_pubkey_file(&lab.user, None, &p, None).is_ok() {
                        break;
                    }
                }
            }
        }
        if !s.authenticated() {
            return Err("AUTH".into());
        }
    }
    if !s.authenticated() {
        return Err("AUTH".into());
    }
    s.set_timeout(0);
    Ok(s)
}

/// Explique en français un échec de connexion.
pub fn friendly(err: &str, lab: &Lab) -> String {
    if err == "AUTH" {
        return format!("Mot de passe refusé pour {}@{}.", lab.user, lab.host);
    }
    if let Some(m) = err.strip_prefix("AUTHMETHOD:") {
        return format!("Cette machine n'accepte pas les mots de passe (méthodes : {m}). Dans sa console, lance :\n    {ENABLE_PASSWORD}");
    }
    err.to_string()
}

/// Cherche pourquoi la machine ne répond pas : port SSH, ping, route par le VPN.
pub fn diagnose(host: &str, port: u16) -> String {
    let tcp = (host, port).to_socket_addrs().ok().and_then(|mut a| a.next())
        .map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(4)));
    match tcp {
        Some(Ok(_)) => return "La machine répond et SSH écoute : vérifie l'utilisateur et le mot de passe.".into(),
        Some(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            return format!("La machine répond, mais SSH n'y est pas démarré. Dans sa console (RemoteLabz), lance :\n    {ENABLE_SSH}");
        }
        _ => {}
    }
    let ping = {
        let mut c = std::process::Command::new("ping");
        if cfg!(windows) { c.args(["-n", "1", "-w", "2000", host]); } else { c.args(["-c", "1", "-W", "2", host]); }
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
        c.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
    };
    let mut dev = String::new();
    if cfg!(unix) {
        if let Ok(o) = std::process::Command::new("ip").args(["-4", "route", "get", host]).output() {
            let t = String::from_utf8_lossy(&o.stdout).to_string();
            if let Some(i) = t.find(" dev ") {
                dev = t[i + 5..].split_whitespace().next().unwrap_or("").to_string();
            }
        }
    }
    let via_vpn = ["tun", "tap", "wg", "ppp", "vpn"].iter().any(|p| dev.starts_with(p));
    let private = host.parse::<std::net::Ipv4Addr>().map(|a| a.is_private()).unwrap_or(false);
    if private && !dev.is_empty() && !via_vpn && !ping {
        return format!("Cette adresse ne passe pas par le VPN (interface « {dev} »). Vérifie dans l'onglet VPN que le VPN \
            de CE lab est connecté (chaque lab RemoteLabz a son propre fichier OpenVPN).");
    }
    if ping {
        return format!("{host} répond au ping mais pas en SSH (port {port}) : SSH n'est sans doute pas installé. \
            Dans la console de la machine :\n    {ENABLE_SSH}");
    }
    format!("{host} ne répond ni au ping ni en SSH{}. Vérifie que la machine est démarrée, l'adresse (« ip addr » dans \
        sa console) et que le VPN connecté est celui de ce lab.", if via_vpn { " alors que le VPN est actif" } else { "" })
}

// ---------------------------------------------------------------------- fichiers distants
#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub dir: bool,
}

pub enum Req {
    List(String),
    Read(String),
    Write(String, Vec<u8>),
    Mkdir(String),
    Touch(String),
    Rename(String, String),
    Remove(String),
    Upload(PathBuf, String),
    Download(String, PathBuf),
}

pub enum Resp {
    Connected { home: String, cleaned: Vec<String> },
    Failed(String),
    Listed(String, Result<Vec<RemoteEntry>, String>),
    Read(String, Result<String, String>),
    Done(String),
    Saved(String),
    SaveFailed(String, String),
    Error(String),
    Closed,
}

pub struct Conn {
    pub lab: Lab,
    pub password: String,
    tx: Sender<Req>,
    pub rx: Receiver<Resp>,
    pub home: Option<String>,
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn exec_simple(s: &Session, cmd: &str) -> Result<String, String> {
    let mut ch = s.channel_session().map_err(|e| e.to_string())?;
    ch.exec(cmd).map_err(|e| e.to_string())?;
    let mut out = String::new();
    let _ = ch.read_to_string(&mut out);
    let mut err = String::new();
    let _ = ch.stderr().read_to_string(&mut err);
    let _ = ch.wait_close();
    if ch.exit_status().unwrap_or(1) != 0 && !err.is_empty() {
        return Err(err.trim().to_string());
    }
    Ok(out)
}

fn upload(sftp: &ssh2::Sftp, local: &Path, remote: &str) -> Result<usize, String> {
    if local.is_dir() {
        let _ = sftp.mkdir(Path::new(remote), 0o755);
        let mut n = 0;
        for e in std::fs::read_dir(local).map_err(|e| e.to_string())?.flatten() {
            n += upload(sftp, &e.path(), &join(remote, &e.file_name().to_string_lossy()))?;
        }
        Ok(n)
    } else {
        let data = std::fs::read(local).map_err(|e| e.to_string())?;
        let mut f = sftp.create(Path::new(remote)).map_err(|e| e.to_string())?;
        f.write_all(&data).map_err(|e| e.to_string())?;
        Ok(1)
    }
}

fn download(sftp: &ssh2::Sftp, remote: &str, local: &Path) -> Result<usize, String> {
    let st = sftp.stat(Path::new(remote)).map_err(|e| e.to_string())?;
    if st.is_dir() {
        std::fs::create_dir_all(local).map_err(|e| e.to_string())?;
        let mut n = 0;
        for (p, _) in sftp.readdir(Path::new(remote)).map_err(|e| e.to_string())? {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n += download(sftp, &join(remote, &name), &local.join(&name))?;
        }
        Ok(n)
    } else {
        let mut f = sftp.open(Path::new(remote)).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        std::fs::write(local, data).map_err(|e| e.to_string())?;
        Ok(1)
    }
}

impl Conn {
    pub fn open(lab: Lab, password: String, ctx: eframe::egui::Context) -> Conn {
        let (tx, rx_req) = channel::<Req>();
        let (tx_resp, rx) = channel::<Resp>();
        let (l2, p2) = (lab.clone(), password.clone());
        std::thread::spawn(move || {
            let send = |r: Resp| { let _ = tx_resp.send(r); ctx.request_repaint(); };
            let s = match connect(&l2, &p2) {
                Ok(s) => s,
                Err(e) => { send(Resp::Failed(e)); return; }
            };
            let home = exec_simple(&s, "pwd").map(|h| h.trim().to_string()).unwrap_or_else(|_| "/".into());
            let mut cleaned = Vec::new();
            if l2.cleanup {
                // arrêt des processus restés d'une séance précédente (non bloquant en cas d'échec)
                s.set_timeout(10_000);
                if let Ok(out) = exec_simple(&s, &format!("sh -c {}", q(CLEANUP))) {
                    cleaned = out.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
                }
                s.set_timeout(0);
            }
            send(Resp::Connected { home, cleaned });
            let sftp = match s.sftp() {
                Ok(f) => f,
                Err(e) => { send(Resp::Failed(format!("SFTP indisponible : {e}"))); return; }
            };
            while let Ok(req) = rx_req.recv() {
                let r = match req {
                    Req::List(dir) => {
                        let res = sftp.readdir(Path::new(&dir)).map(|v| {
                            let mut out: Vec<RemoteEntry> = v.into_iter().filter_map(|(p, st)| {
                                let name = p.file_name()?.to_string_lossy().to_string();
                                Some(RemoteEntry { path: join(&dir, &name), dir: st.is_dir(), name })
                            }).collect();
                            out.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
                            out
                        }).map_err(|e| e.to_string());
                        Resp::Listed(dir, res)
                    }
                    Req::Read(path) => {
                        let res = (|| {
                            let st = sftp.stat(Path::new(&path)).map_err(|e| e.to_string())?;
                            if st.size.unwrap_or(0) > 20_000_000 { return Err("fichier trop gros (plus de 20 Mo)".to_string()); }
                            let mut f = sftp.open(Path::new(&path)).map_err(|e| e.to_string())?;
                            let mut data = Vec::new();
                            f.read_to_end(&mut data).map_err(|e| e.to_string())?;
                            if data.iter().take(8000).any(|&b| b == 0) { return Err("fichier binaire".to_string()); }
                            Ok(String::from_utf8_lossy(&data).replace("\r\n", "\n"))
                        })();
                        Resp::Read(path, res)
                    }
                    Req::Write(path, data) => match sftp.create(Path::new(&path)).map_err(|e| e.to_string())
                        .and_then(|mut f| f.write_all(&data).map_err(|e| e.to_string())) {
                        Ok(()) => Resp::Saved(path),
                        Err(e) => Resp::SaveFailed(path, e),
                    },
                    Req::Mkdir(p) => match sftp.mkdir(Path::new(&p), 0o755) {
                        Ok(()) => Resp::Done(String::new()), Err(e) => Resp::Error(format!("Création impossible : {e}")) },
                    Req::Touch(p) => match sftp.create(Path::new(&p)) {
                        Ok(_) => Resp::Done(String::new()), Err(e) => Resp::Error(format!("Création impossible : {e}")) },
                    Req::Rename(a, b) => match sftp.rename(Path::new(&a), Path::new(&b), None) {
                        Ok(()) => Resp::Done(String::new()), Err(e) => Resp::Error(format!("Renommage impossible : {e}")) },
                    Req::Remove(p) => match exec_simple(&s, &format!("rm -rf -- {}", q(&p))) {
                        Ok(_) => Resp::Done(String::new()), Err(e) => Resp::Error(format!("Suppression impossible : {e}")) },
                    Req::Upload(local, remote) => match upload(&sftp, &local, &remote) {
                        Ok(n) => Resp::Done(format!("{n} fichier(s) envoyé(s) sur {}", l2.label())),
                        Err(e) => Resp::Error(format!("Envoi impossible : {e}")) },
                    Req::Download(remote, local) => match download(&sftp, &remote, &local) {
                        Ok(n) => Resp::Done(format!("{n} fichier(s) téléchargé(s) dans {}", local.display())),
                        Err(e) => Resp::Error(format!("Téléchargement impossible : {e}")) },
                };
                send(r);
            }
            send(Resp::Closed);
        });
        Conn { lab, password, tx, rx, home: None }
    }

    pub fn send(&self, r: Req) {
        let _ = self.tx.send(r);
    }
}

// ---------------------------------------------------------------------- exécution distante
pub struct RemoteRun {
    rx: Receiver<Out>,
    stdin: Sender<Vec<u8>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    exit: std::sync::Arc<std::sync::Mutex<Option<Option<i32>>>>,
    finished: bool,
}

impl RemoteRun {
    pub fn start(lab: Lab, password: String, cmdline: String, ctx: eframe::egui::Context) -> RemoteRun {
        let (tx, rx) = channel();
        let (stdin, rx_in) = channel::<Vec<u8>>();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let exit = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (stop2, exit2) = (stop.clone(), exit.clone());
        std::thread::spawn(move || {
            let send = |o: Out| { let _ = tx.send(o); ctx.request_repaint(); };
            let fail = |m: String| { let _ = tx.send(Out::Stderr(m)); };
            let s = match connect(&lab, &password) {
                Ok(s) => s,
                Err(e) => { fail(format!("{}\n", friendly(&e, &lab))); *exit2.lock().unwrap() = Some(Some(255)); ctx.request_repaint(); return; }
            };
            let mut ch = match s.channel_session() { Ok(c) => c, Err(e) => { fail(e.to_string()); *exit2.lock().unwrap() = Some(Some(255)); return; } };
            if let Err(e) = ch.exec(&cmdline) { fail(e.to_string()); *exit2.lock().unwrap() = Some(Some(255)); return; }
            s.set_blocking(false);
            let mut buf = [0u8; 8192];
            loop {
                let mut idle = true;
                if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = ch.close();
                    *exit2.lock().unwrap() = Some(None);
                    break;
                }
                match ch.read(&mut buf) {
                    Ok(n) if n > 0 => { idle = false; send(Out::Stdout(String::from_utf8_lossy(&buf[..n]).to_string())); }
                    _ => {}
                }
                match ch.stderr().read(&mut buf) {
                    Ok(n) if n > 0 => { idle = false; send(Out::Stderr(String::from_utf8_lossy(&buf[..n]).to_string())); }
                    _ => {}
                }
                loop {
                    match rx_in.try_recv() {
                        Ok(d) => { idle = false; let mut off = 0; while off < d.len() {
                            match ch.write(&d[off..]) { Ok(n) => off += n, Err(_) => std::thread::sleep(Duration::from_millis(5)) } } }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => break,
                    }
                }
                if ch.eof() {
                    s.set_blocking(true);
                    let _ = ch.wait_close();
                    *exit2.lock().unwrap() = Some(ch.exit_status().ok());
                    ctx.request_repaint();
                    break;
                }
                if idle { std::thread::sleep(Duration::from_millis(15)); }
            }
        });
        RemoteRun { rx, stdin, stop, exit, finished: false }
    }
}

impl Proc for RemoteRun {
    fn try_recv(&mut self) -> Option<Out> {
        self.rx.try_recv().ok()
    }
    fn write(&mut self, text: &str) {
        let _ = self.stdin.send(text.as_bytes().to_vec());
    }
    fn kill(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    fn poll_exit(&mut self) -> Option<Option<i32>> {
        if self.finished { return None; }
        let e = *self.exit.lock().unwrap();
        if let Some(code) = e {
            self.finished = true;
            return Some(code);
        }
        None
    }
    fn finished(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse() {
        let l = quick_parse("ubuntu@10.11.0.2").unwrap();
        assert_eq!((l.user.as_str(), l.host.as_str(), l.port), ("ubuntu", "10.11.0.2", 22));
        let l = quick_parse("Serveur = root@10.0.0.1:2222").unwrap();
        assert_eq!((l.name.as_str(), l.user.as_str(), l.port), ("Serveur", "root", 2222));
        assert_eq!(l.label(), "Serveur");
        let l = quick_parse("10.0.0.5").unwrap();
        assert_eq!(l.user, "ubuntu");
        assert!(quick_parse("").is_none());
        assert!(quick_parse("a b@c").is_none());
    }
}

/// Tests contre un vrai serveur SSH de test (lancé par sshtest/run_ssh_server.py) :
/// SC_TEST_SSH_PORT=xxxx cargo test -- --ignored ssh_live
#[cfg(test)]
mod live {
    use super::*;
    fn lab() -> Option<Lab> {
        let port: u16 = std::env::var("SC_TEST_SSH_PORT").ok()?.parse().ok()?;
        Some(Lab { name: "Test".into(), user: "etu".into(), host: "127.0.0.1".into(), port, cleanup: false, ..Default::default() })
    }
    fn wait(c: &Conn) -> Resp {
        c.rx.recv_timeout(Duration::from_secs(15)).expect("pas de réponse")
    }
    #[test]
    #[ignore]
    fn ssh_live() {
        let Some(lab) = lab() else { return };
        let home = std::env::var("SC_TEST_SSH_HOME").unwrap();
        // mauvais mot de passe
        assert_eq!(connect(&lab, "faux").err().as_deref(), Some("AUTH"));
        let ctx = eframe::egui::Context::default();
        let c = Conn::open(lab.clone(), "secret".into(), ctx.clone());
        match wait(&c) { Resp::Connected { home: h, .. } => assert_eq!(h, home), _ => panic!("connexion") }
        c.send(Req::Write(format!("{home}/essai.py"), b"print('bonjour', input())\n".to_vec()));
        assert!(matches!(wait(&c), Resp::Saved(_)));
        c.send(Req::List(home.clone()));
        match wait(&c) { Resp::Listed(_, Ok(v)) => assert!(v.iter().any(|e| e.name == "essai.py")), _ => panic!("liste") }
        c.send(Req::Read(format!("{home}/essai.py")));
        match wait(&c) { Resp::Read(_, Ok(t)) => assert!(t.contains("bonjour")), _ => panic!("lecture") }
        c.send(Req::Mkdir(format!("{home}/dossier")));
        assert!(matches!(wait(&c), Resp::Done(_)));
        c.send(Req::Rename(format!("{home}/essai.py"), format!("{home}/dossier/essai.py")));
        assert!(matches!(wait(&c), Resp::Done(_)));
        let tmp = std::env::temp_dir().join("sc-up");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("a.txt"), "A").unwrap();
        c.send(Req::Upload(tmp.clone(), format!("{home}/up")));
        match wait(&c) { Resp::Done(m) => assert!(m.contains("1 fichier"), "{m}"), _ => panic!("envoi") }
        let down = std::env::temp_dir().join("sc-down");
        let _ = std::fs::remove_dir_all(&down);
        c.send(Req::Download(format!("{home}/up"), down.clone()));
        assert!(matches!(wait(&c), Resp::Done(_)));
        assert_eq!(std::fs::read_to_string(down.join("a.txt")).unwrap(), "A");
        // exécution distante avec saisie
        let line = crate::runner::remote_command(&format!("{home}/dossier/essai.py"), &[]).unwrap();
        let mut r = RemoteRun::start(lab.clone(), "secret".into(), line, ctx);
        r.write("Anto\n");
        let mut out = String::new();
        let t = std::time::Instant::now();
        loop {
            while let Some(o) = r.try_recv() { if let Out::Stdout(s) | Out::Stderr(s) = o { out.push_str(&s); } }
            if let Some(code) = r.poll_exit() {
                std::thread::sleep(Duration::from_millis(100));
                while let Some(o) = r.try_recv() { if let Out::Stdout(s) | Out::Stderr(s) = o { out.push_str(&s); } }
                assert_eq!(code, Some(0), "{out}");
                break;
            }
            assert!(t.elapsed().as_secs() < 20, "trop long : {out}");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(out.contains("bonjour Anto"), "{out}");
        c.send(Req::Remove(format!("{home}/dossier")));
        assert!(matches!(wait(&c), Resp::Done(_)));
        // diagnostic d'un port fermé
        assert!(diagnose("127.0.0.1", 1).contains("SSH n'y est pas démarré"));
    }
}

#[cfg(test)]
mod live_shell {
    use super::*;
    #[test]
    #[ignore]
    fn ssh_shell() {
        let port: u16 = std::env::var("SC_TEST_SSH_PORT").unwrap().parse().unwrap();
        let lab = Lab { user: "etu".into(), host: "127.0.0.1".into(), port, cleanup: false, ..Default::default() };
        let s = connect(&lab, "secret").unwrap();
        let mut ch = s.channel_session().unwrap();
        ch.request_pty("xterm-256color", None, Some((80, 24, 0, 0))).unwrap();
        ch.shell().unwrap();
        s.set_blocking(false);
        let mut buf = [0u8; 4096];
        let mut out = Vec::new();
        let msg = b"echo bonjour\n";
        let mut off = 0;
        let t = std::time::Instant::now();
        while t.elapsed().as_secs() < 5 {
            if off < msg.len() {
                match ch.write(&msg[off..]) { Ok(n) => { off += n; eprintln!("écrit {n}"); } Err(e) => eprintln!("écriture : {e:?}") }
            }
            match ch.read(&mut buf) { Ok(n) if n > 0 => { out.extend_from_slice(&buf[..n]); } Ok(_) => {} Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {} Err(e) => eprintln!("lecture : {e:?}") }
            if String::from_utf8_lossy(&out).contains("bonjour") { break; }
            std::thread::sleep(Duration::from_millis(20));
        }
        eprintln!("sortie : {:?}", String::from_utf8_lossy(&out));
        assert!(String::from_utf8_lossy(&out).contains("bonjour"));
    }
}

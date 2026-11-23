//! Statut d'activité Discord (« Rich Presence »), comme les extensions de VS Code : Discord montre
//! qu'on est sur SCode, le fichier ouvert, son langage et depuis combien de temps.
//!
//! On parle directement au client Discord installé sur l'ordinateur, par son canal local
//! (« discord-ipc-0 » : socket Unix sous Linux/macOS, tube nommé sous Windows). Tout se passe dans
//! un fil d'exécution à part : si Discord n'est pas lancé, rien ne se passe et on réessaie de temps
//! en temps, sans jamais gêner l'éditeur.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Identifiant de l'application Discord « SCode » (Discord Developer Portal → Applications →
/// General Information → Application ID). Le nom de l'application est celui qu'affiche Discord
/// (« Joue à SCode »). Peut être remplacé dans settings.json (« discord_app_id »).
pub const DEFAULT_APP_ID: &str = "";

/// Image affichée dans le statut (Discord accepte une adresse https).
const LOGO_URL: &str = "https://raw.githubusercontent.com/AntoninMichelq/studentCode/main/rust/assets/icon-256.png";

/// Ce que l'éditeur veut montrer ; `None` = rien (réglage désactivé).
#[derive(Clone, PartialEq, Debug)]
pub struct Activity {
    pub app_id: String,
    /// première ligne (« Modifie main.py »)
    pub details: String,
    /// deuxième ligne (« Python · dossier projet »)
    pub state: String,
}

pub struct Presence {
    tx: Sender<Option<Activity>>,
    last: Option<Option<Activity>>,
}

impl Presence {
    pub fn start() -> Presence {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("discord".into()).spawn(move || worker(rx));
        Presence { tx, last: None }
    }

    /// À appeler à chaque image : n'envoie quelque chose que si le statut a changé.
    pub fn set(&mut self, a: Option<Activity>) {
        if self.last.as_ref() == Some(&a) { return; }
        self.last = Some(a.clone());
        let _ = self.tx.send(a);
    }
}

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

struct Conn {
    io: Box<dyn Pipe>,
    app_id: String,
    nonce: u64,
}

fn worker(rx: Receiver<Option<Activity>>) {
    let started = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut want: Option<Activity> = None;
    let mut conn: Option<Conn> = None;
    // l'activité est-elle déjà affichée sur la connexion actuelle ?
    let mut sent = false;
    let mut last_try: Option<Instant> = None;
    loop {
        // on se réveille de temps en temps pour retenter la connexion si Discord était fermé
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(a) => {
                // on prend la plus récente des demandes en attente
                want = a;
                while let Ok(a) = rx.try_recv() { want = a; }
                sent = false;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(c) = conn.as_mut() { let _ = c.clear(); }
                return;
            }
        }
        let Some(a) = want.clone() else {
            // désactivé : on efface le statut et on ferme
            if let Some(mut c) = conn.take() { let _ = c.clear(); }
            continue;
        };
        if a.app_id.trim().is_empty() { conn = None; continue; }
        if conn.as_ref().map(|c| c.app_id != a.app_id).unwrap_or(false) { conn = None; }
        if conn.is_none() {
            if last_try.map(|t| t.elapsed() < Duration::from_secs(10)).unwrap_or(false) { continue; }
            last_try = Some(Instant::now());
            conn = Conn::open(a.app_id.trim());
            sent = false;
        }
        if sent { continue; }
        if let Some(c) = conn.as_mut() {
            if c.set(&a, started).is_ok() { sent = true; } else { conn = None; }
        }
    }
}

impl Conn {
    fn open(app_id: &str) -> Option<Conn> {
        for io in candidates() {
            let mut c = Conn { io, app_id: app_id.to_string(), nonce: 0 };
            if c.write(0, &json!({ "v": 1, "client_id": app_id })).is_ok() {
                // Discord répond « READY » ; une erreur (identifiant inconnu) ferme la connexion
                match c.read() {
                    Ok((op, v)) if op == 1 && v.get("evt").and_then(Value::as_str) == Some("READY") => return Some(c),
                    _ => continue,
                }
            }
        }
        None
    }

    fn set(&mut self, a: &Activity, started: u64) -> std::io::Result<()> {
        let activity = json!({
            "type": 0,
            "details": fit(&a.details),
            "state": fit(&a.state),
            "timestamps": { "start": started },
            "assets": { "large_image": LOGO_URL, "large_text": "SCode" },
        });
        self.command(json!({ "pid": std::process::id(), "activity": activity }))
    }

    fn clear(&mut self) -> std::io::Result<()> {
        self.command(json!({ "pid": std::process::id(), "activity": Value::Null }))
    }

    fn command(&mut self, args: Value) -> std::io::Result<()> {
        self.nonce += 1;
        self.write(1, &json!({ "cmd": "SET_ACTIVITY", "args": args, "nonce": self.nonce.to_string() }))?;
        // on lit la réponse pour ne pas laisser le canal se remplir
        let (_, v) = self.read()?;
        if v.get("evt").and_then(Value::as_str) == Some("ERROR") {
            return Err(std::io::Error::other(v.to_string()));
        }
        Ok(())
    }

    fn write(&mut self, op: u32, v: &Value) -> std::io::Result<()> {
        let body = v.to_string();
        let mut buf = Vec::with_capacity(8 + body.len());
        buf.extend_from_slice(&op.to_le_bytes());
        buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend_from_slice(body.as_bytes());
        self.io.write_all(&buf)?;
        self.io.flush()
    }

    fn read(&mut self) -> std::io::Result<(u32, Value)> {
        let mut head = [0u8; 8];
        self.io.read_exact(&mut head)?;
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if len > 1 << 20 { return Err(std::io::Error::other("réponse de Discord trop grande")); }
        let mut body = vec![0u8; len];
        self.io.read_exact(&mut body)?;
        if op == 2 { return Err(std::io::Error::other("Discord a fermé la connexion")); }
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }
}

/// Discord veut entre 2 et 128 caractères.
fn fit(s: &str) -> String {
    let mut s: String = s.chars().take(128).collect();
    while s.chars().count() < 2 { s.push(' '); }
    s
}

/// Canaux possibles du client Discord (discord-ipc-0 à 9).
#[cfg(unix)]
fn candidates() -> Vec<Box<dyn Pipe>> {
    use std::os::unix::net::UnixStream;
    let mut dirs = Vec::new();
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(d) = std::env::var_os(var) { dirs.push(std::path::PathBuf::from(d)); }
    }
    dirs.push("/tmp".into());
    // Discord installé en Flatpak ou en Snap
    let mut all = Vec::new();
    for d in &dirs {
        all.push(d.clone());
        all.push(d.join("app/com.discordapp.Discord"));
        all.push(d.join("app/com.discordapp.DiscordCanary"));
        all.push(d.join("snap.discord"));
        all.push(d.join(".flatpak/dev.vencord.Vesktop/xdg-run"));
    }
    let mut out: Vec<Box<dyn Pipe>> = Vec::new();
    for d in all {
        for i in 0..10 {
            if let Ok(s) = UnixStream::connect(d.join(format!("discord-ipc-{i}"))) {
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
                out.push(Box::new(s));
            }
        }
    }
    out
}

#[cfg(windows)]
fn candidates() -> Vec<Box<dyn Pipe>> {
    (0..10).filter_map(|i| {
        std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok()
            .map(|f| Box::new(f) as Box<dyn Pipe>)
    }).collect()
}

#[cfg(not(any(unix, windows)))]
fn candidates() -> Vec<Box<dyn Pipe>> { Vec::new() }

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn frame(s: &mut impl Read) -> (u32, Value) {
        let mut h = [0u8; 8];
        s.read_exact(&mut h).unwrap();
        let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
        let mut b = vec![0u8; len];
        s.read_exact(&mut b).unwrap();
        (u32::from_le_bytes([h[0], h[1], h[2], h[3]]), serde_json::from_slice(&b).unwrap())
    }

    fn reply(s: &mut impl Write, op: u32, v: Value) {
        let b = v.to_string();
        s.write_all(&op.to_le_bytes()).unwrap();
        s.write_all(&(b.len() as u32).to_le_bytes()).unwrap();
        s.write_all(b.as_bytes()).unwrap();
    }

    /// Faux client Discord : poignée de main puis SET_ACTIVITY avec le fichier et le langage.
    #[test]
    fn handshake_and_activity() {
        let dir = std::env::temp_dir().join(format!("scode-discord-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let sock = dir.join("discord-ipc-0");
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir); }
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let (op, hello) = frame(&mut s);
            assert_eq!(op, 0);
            assert_eq!(hello["client_id"], "123");
            reply(&mut s, 1, json!({ "cmd": "DISPATCH", "evt": "READY" }));
            let (op, cmd) = frame(&mut s);
            reply(&mut s, 1, json!({ "cmd": "SET_ACTIVITY", "nonce": cmd["nonce"] }));
            (op, cmd)
        });
        let mut p = Presence::start();
        p.set(Some(Activity { app_id: "123".into(), details: "Modifie main.py".into(), state: "Python · dossier tp1".into() }));
        let (op, cmd) = server.join().unwrap();
        assert_eq!(op, 1);
        assert_eq!(cmd["cmd"], "SET_ACTIVITY");
        assert_eq!(cmd["args"]["activity"]["details"], "Modifie main.py");
        assert_eq!(cmd["args"]["activity"]["state"], "Python · dossier tp1");
        assert!(cmd["args"]["activity"]["timestamps"]["start"].as_u64().unwrap() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

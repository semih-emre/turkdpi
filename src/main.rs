mod nft;
mod profile;

use anyhow::{bail, Context, Result};
use profile::{Profile, ProfileName};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const RUN_DIR: &str = "/run/turkdpi";
const STATE_DIR: &str = "/var/lib/turkdpi";
const PROFILE_DIR: &str = "/usr/share/turkdpi/profiles";
const NFQWS: &str = "/usr/lib/turkdpi/nfqws";

#[derive(Serialize)]
struct Status<'a> {
    active: bool,
    profile: &'a str,
    method: &'a str,
    message: &'a str,
}

fn require_root() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("bu işlem root yetkisi gerektirir (Polkit/pkexec kullanın)");
    }
    Ok(())
}

fn atomic_status(status: &Status<'_>) -> Result<()> {
    fs::create_dir_all(RUN_DIR)?;
    let target = Path::new(RUN_DIR).join("status.json");
    let temp = Path::new(RUN_DIR).join("status.json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o644)
        .open(&temp)?;
    serde_json::to_writer_pretty(&mut file, status)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(temp, target)?;
    Ok(())
}

fn load_profile(name: ProfileName) -> Result<Profile> {
    let path = Path::new(PROFILE_DIR).join(format!("{}.toml", name.as_str()));
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("profil okunamadı: {}", path.display()))?;
    let parsed: Profile = toml::from_str(&raw).context("profil TOML biçimi geçersiz")?;
    parsed.validate(name)?;
    Ok(parsed)
}

fn spawn_engine(queue: u16, profile: &Profile, udp: bool) -> Result<u32> {
    let mut cmd = Command::new(NFQWS);
    cmd.arg(format!("--qnum={queue}"))
        .arg("--user=nobody")
        .arg("--debug=syslog")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for arg in if udp {
        &profile.udp_args
    } else {
        &profile.tcp_args
    } {
        cmd.arg(arg);
    }
    if !udp {
        cmd.arg(format!("--hostlist={}", profile.hostlist));
    }
    let mut child = cmd.spawn().context("nfqws başlatılamadı")?;
    std::thread::sleep(Duration::from_millis(150));
    if let Some(status) = child.try_wait()? {
        bail!("nfqws erken sonlandı: {status}");
    }
    Ok(child.id())
}

fn pid_file() -> PathBuf {
    Path::new(RUN_DIR).join("nfqws.pids")
}

fn start(name: ProfileName) -> Result<()> {
    require_root()?;
    nft::backup_ruleset(Path::new(STATE_DIR))?;
    cleanup_inner()?;
    let profile = load_profile(name)?;
    nft::apply()?;
    let tcp_pid = match spawn_engine(200, &profile, false) {
        Ok(pid) => pid,
        Err(error) => {
            let _ = nft::cleanup();
            return Err(error);
        }
    };
    let udp_pid = match spawn_engine(201, &profile, true) {
        Ok(pid) => pid,
        Err(error) => {
            unsafe {
                libc::kill(tcp_pid as i32, libc::SIGTERM);
            }
            let _ = nft::cleanup();
            return Err(error);
        }
    };
    fs::write(pid_file(), format!("{tcp_pid}\n{udp_pid}\n"))?;
    atomic_status(&Status {
        active: true,
        profile: name.as_str(),
        method: &profile.description,
        message: "nfqws ve turkdpi nftables tablosu etkin",
    })
}

fn stop_pid(pid: i32) {
    if pid > 1 {
        let proc_exe = fs::read_link(format!("/proc/{pid}/exe")).ok();
        if proc_exe.as_deref() == Some(Path::new(NFQWS)) {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
}

fn cleanup_inner() -> Result<()> {
    let mut pids = Vec::new();
    if let Ok(raw) = fs::read_to_string(pid_file()) {
        for line in raw.lines() {
            if let Ok(pid) = line.parse::<i32>() {
                stop_pid(pid);
                pids.push(pid);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(250));
    for pid in pids {
        let proc_exe = fs::read_link(format!("/proc/{pid}/exe")).ok();
        if proc_exe.as_deref() == Some(Path::new(NFQWS)) {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
    let _ = fs::remove_file(pid_file());
    nft::cleanup()?;
    atomic_status(&Status {
        active: false,
        profile: "none",
        method: "none",
        message: "turkdpi kuralları temizlendi",
    })
}

fn cleanup() -> Result<()> {
    require_root()?;
    nft::backup_ruleset(Path::new(STATE_DIR))?;
    cleanup_inner()
}

fn active_network_uuid() -> Option<String> {
    let out = Command::new("/usr/bin/nmcli")
        .args(["-g", "UUID", "connection", "show", "--active"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let uuid = String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .to_owned();
    if uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        Some(uuid)
    } else {
        None
    }
}

fn connectivity_test() -> Result<String> {
    let timeout = Duration::from_secs(5);
    let dns = ("discord.com", 443)
        .to_socket_addrs()?
        .next()
        .context("Discord DNS yanıtı yok")?;
    TcpStream::connect_timeout(&dns, timeout).context("Discord HTTPS TCP bağlantısı başarısız")?;
    let gateway = ("gateway.discord.gg", 443)
        .to_socket_addrs()?
        .next()
        .context("Gateway DNS yanıtı yok")?;
    TcpStream::connect_timeout(&gateway, timeout)
        .context("Discord Gateway TCP bağlantısı başarısız")?;
    for url in ["https://discord.com/", "https://discord.com/api/gateway"] {
        let result = Command::new("/usr/bin/curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--max-time",
                "8",
                "--output",
                "/dev/null",
                url,
            ])
            .output()
            .context("curl çalıştırılamadı")?;
        if !result.status.success() {
            bail!(
                "HTTPS testi başarısız: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    let udp = UdpSocket::bind("0.0.0.0:0")?;
    udp.connect("1.1.1.1:443")?;
    udp.send(&[0u8])?;
    Ok(
        "Discord DNS/HTTPS ve Gateway TCP başarılı; UDP gönderimi mümkün (yanıt doğrulanmadı)"
            .into(),
    )
}

fn auto_test() -> Result<()> {
    require_root()?;
    let network_uuid = active_network_uuid();
    if let Some(uuid) = &network_uuid {
        let saved = Path::new(STATE_DIR)
            .join("networks")
            .join(format!("{uuid}.profile"));
        if let Ok(name) = fs::read_to_string(saved) {
            if let Ok(candidate) = name.trim().parse::<ProfileName>() {
                start(candidate)?;
                std::thread::sleep(Duration::from_millis(800));
                if connectivity_test().is_ok() {
                    return Ok(());
                }
            }
        }
    }
    for candidate in [
        ProfileName::Safe,
        ProfileName::Discord,
        ProfileName::Balanced,
        ProfileName::Aggressive,
    ] {
        start(candidate)?;
        std::thread::sleep(Duration::from_millis(800));
        if let Ok(message) = connectivity_test() {
            fs::create_dir_all(STATE_DIR)?;
            fs::write(
                Path::new(STATE_DIR).join("selected-profile"),
                candidate.as_str(),
            )?;
            if let Some(uuid) = &network_uuid {
                let dir = Path::new(STATE_DIR).join("networks");
                fs::create_dir_all(&dir)?;
                fs::write(dir.join(format!("{uuid}.profile")), candidate.as_str())?;
            }
            atomic_status(&Status {
                active: true,
                profile: candidate.as_str(),
                method: "otomatik seçildi",
                message: &message,
            })?;
            return Ok(());
        }
    }
    cleanup_inner()?;
    bail!("hiçbir profil bağlantı testini geçemedi")
}

fn status() -> Result<()> {
    let path = Path::new(RUN_DIR).join("status.json");
    if path.exists() {
        print!("{}", fs::read_to_string(path)?);
    } else {
        println!("{{\"active\":false,\"profile\":\"none\",\"method\":\"none\",\"message\":\"henüz çalıştırılmadı\"}}");
    }
    Ok(())
}

fn set_profile(name: ProfileName) -> Result<()> {
    require_root()?;
    fs::create_dir_all(STATE_DIR)?;
    fs::write(Path::new(STATE_DIR).join("selected-profile"), name.as_str())?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "stop" || cmd == "cleanup" => cleanup(),
        [cmd] if cmd == "status" => status(),
        [cmd] if cmd == "test" => { println!("{}", connectivity_test()?); Ok(()) },
        [cmd] if cmd == "auto" => auto_test(),
        [cmd, name] if cmd == "start" => start(name.parse()?),
        [cmd, name] if cmd == "set-profile" => set_profile(name.parse()?),
        _ => bail!("kullanım: turkdpi-service start <safe|balanced|discord|aggressive> | stop | test | auto | cleanup | status | set-profile <profil>"),
    }
}

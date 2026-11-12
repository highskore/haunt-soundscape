//! End to end against a real haunt core. Copy into haunt's `cli/tests/` and run
//! `cargo test -p haunt-cli --features plugins --test soundscape` (see README).
#![cfg(feature = "plugins")]

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use haunt_core::plugin_settings::PluginConfig;
use haunt_core::plugin_store::{PluginSet, Store, attach};
use haunt_core::plugins::{HostRequest, PluginHost};
use haunt_core::{Core, SurfaceEnv};

const ID: &str = "dev.haunt.soundscape";

fn repo() -> PathBuf {
    std::env::var_os("HAUNT_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../haunt-plugins"))
        .join("haunt-soundscape")
}

fn send(sock: &mut UnixStream, env: &SurfaceEnv, nonce: bool, rest: &str) {
    let nonce = if nonce { format!(r#","nonce":"{}""#, env.nonce) } else { String::new() };
    let line = format!(r#"{{"v":1,"agent":"claude","surface_id":"{}"{nonce},"session_id":"s1","cwd":"/tmp/api",{rest}}}"#, env.surface_id);
    sock.write_all(format!("{line}\n").as_bytes()).unwrap();
}

/// The plugin's sound requests so far, as text: `play Chime 3 0.50`, `ambient Rain 0.08`.
fn sounds(set: &Mutex<PluginSet>, want: usize) -> Vec<String> {
    let t = Instant::now();
    let mut out = Vec::new();
    while out.len() < want {
        out.extend(set.lock().unwrap().take_requests().into_iter().filter_map(|r| match r {
            HostRequest::SoundPlay { plugin_id, sound, pitch, volume } if plugin_id == ID => Some(format!("play {sound:?} {pitch} {volume:.2}")),
            HostRequest::SoundAmbient { plugin_id, sound, level } if plugin_id == ID => Some(format!("ambient {sound:?} {level:.2}")),
            _ => None,
        }));
        assert!(t.elapsed() < Duration::from_secs(5), "timed out waiting for {want} sounds: {out:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    out
}

#[test]
fn agents_chime_when_they_need_you_and_rain_while_they_run() {
    let dir = repo();
    if !dir.join("haunt-plugin.toml").is_file() {
        eprintln!("skipped: haunt-soundscape not found; set HAUNT_PLUGINS_DIR");
        return;
    }
    let home = PathBuf::from(format!("/tmp/haunt-fp-sound-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--release", "--target", "wasm32-wasip2"])
        .current_dir(&dir)
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .unwrap();
    assert!(status.success());
    let o = Command::new(env!("CARGO_BIN_EXE_haunt")).args(["plugin", "install", dir.to_str().unwrap(), "--yes"]).env("HAUNT_HOME", &home).output().unwrap();
    assert!(o.status.success(), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));

    let core = Core::new(&home);
    core.start().unwrap();
    let (env, other) = (core.register_surface("/tmp").unwrap(), core.register_surface("/tmp").unwrap());
    let set = Store::open(&home).unwrap().load_all(&PluginHost::new().unwrap(), Some(&core.handle())).unwrap();
    assert!(set.errors().is_empty(), "{:?}", set.errors());
    let set = Arc::new(Mutex::new(set));
    attach(&core, &set);

    let mut sock = UnixStream::connect(&env.socket_path).unwrap();
    send(&mut sock, &env, true, r#""event":"prompt_submit","query":"go""#);
    send(&mut sock, &other, false, r#""event":"permission_seen","tool_name":"Bash""#);
    send(&mut sock, &env, true, r#""event":"permission_seen","tool_name":"Bash""#);
    let s = sounds(&set, 1);
    assert_eq!(s.len(), 1, "one chime, none for the forged ask: {s:?}");
    assert!(s[0].starts_with("play Chime ") && s[0].ends_with(" 0.50"), "{s:?}");
    send(&mut sock, &env, true, r#""event":"permission_seen","tool_name":"Bash""#);
    send(&mut sock, &env, true, r#""event":"prompt_submit","query":"yes""#);
    send(&mut sock, &env, true, r#""event":"stop""#);
    assert_eq!(sounds(&set, 1), ["play Bell 0 0.50"], "one chime per wait, then a bell when it's done");

    set.lock().unwrap().set_config(&PluginConfig::parse(&format!("[plugins.\"{ID}\"]\nrain = true\nrain_full_at = 2\nvolume = 100\n")));
    send(&mut sock, &env, true, r#""event":"prompt_submit","query":"more""#);
    assert_eq!(sounds(&set, 1), ["ambient Rain 0.50"], "one of two agents for full rain");
    send(&mut sock, &env, true, r#""event":"stop""#);
    assert_eq!(sounds(&set, 2), ["play Bell 0 1.00", "ambient Rain 0.00"]);

    drop(core);
    let _ = std::fs::remove_dir_all(&home);
}

use crate::command::output_format_handler::NetworkLayer;
use std::env;
use std::ffi::OsStr;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

const DISSECTOR: &str = include_str!("../../wireshark/nr_dissector.lua");

pub fn launch_wireshark<T: AsRef<OsStr> + ?Sized>(path: &T, dev: bool, network_layer: NetworkLayer) -> Result<String, String> {
    // If we were executed with root the dissector script will not work!
    if started_with_special_privs() {
        return Err("Wireshark can not be launched when running as root or changing permissions under the root account. If you require CAP_NET_ADMIN permissions, attempt to add permissions to the binary instead using: \"sudo setcap cap_net_admin+ep <path/to/descrambler>\"".to_string());
    }

    // Create temp dissector script
    let mut tmp_path = env::temp_dir();
    tmp_path.push(format!("tmpfile_{}.lua", rand::random::<u32>()));

    std::fs::write(&tmp_path, DISSECTOR).map_err(|e| format!("Error writing wireshark dissector to {tmp_path:?}: {}", e))?;

    let p = tmp_path.to_str().ok_or("Temp path for dissector contains invalid UTF-8".to_string())?;

    // Run wireshark
    let mut cmd = Command::new("wireshark");
    cmd.env("WIRESHARK_LUA_DEBUG", "1");
    if dev {
        cmd.arg("-i");
        cmd.arg(path);
        cmd.arg("-k");
    } else {
        cmd.arg(path);
    }
    cmd.arg("--only-protocols");
    match network_layer {
        NetworkLayer::Layer2 => {
            cmd.arg("eth,ip,udp,mac-nr,nr-rrc,data,nr-ssb,nr-dci");
        }
        NetworkLayer::Layer3 => {
            cmd.arg("raw,ip,udp,mac-nr,nr-rrc,data,nr-ssb,nr-dci");
        }
    }
    cmd.arg("-X");
    cmd.arg(format!("lua_script:{p}"));
    cmd.arg("-Y");
    cmd.arg("nr-rrc || nr-ssb || nr-dci");
    cmd.arg("--enable-heuristic");
    cmd.arg("mac_nr_udp");

    // Ensure that CTRL+C to abort us is not forwarded to wireshark
    #[cfg(target_family = "unix")]
    unsafe {
        use std::os::unix::prelude::CommandExt;
        cmd.pre_exec(|| {
            // Create new session so process is not associated with us
            libc::setsid();
            Ok(())
        });
    }
    #[cfg(target_family = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // See https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;

        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    // Hide output of wireshark and spawn
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn().map_err(|e| format!("{e}"))?;

    // Sleep short amount of time to allow wireshark to start capturing when capturing a device
    if dev {
        sleep(Duration::from_millis(2000));
    }

    Ok(format!("{:?}", cmd.get_args().collect::<Vec<_>>()))
}

// Function mimicking the wireshark counterpart at
// https://github.com/wireshark/wireshark/blob/f0c83cd5479f3654c442e491e298bcd84c234246/wsutil/privileges.c#L150
fn started_with_special_privs() -> bool {
    #[cfg(target_family = "unix")]
    {
        // Check if uid/gid has been changed or is equals to 0 (root)
        let ruid = unsafe { libc::getuid() };
        let euid = unsafe { libc::geteuid() };
        let rgid = unsafe { libc::getgid() };
        let egid = unsafe { libc::getegid() };

        ruid != euid || rgid != egid || ruid == 0 || rgid == 0
    }

    #[cfg(not(target_family = "unix"))]
    {
        false
    }
}
use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy_to_clipboard(text: &str) {
    let (cmd, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("pbcopy", &[])
    } else if cfg!(target_os = "linux") {
        ("xclip", &["-selection", "clipboard"])
    } else if cfg!(target_os = "windows") {
        ("clip", &[])
    } else {
        return;
    };

    if let Ok(mut child) = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

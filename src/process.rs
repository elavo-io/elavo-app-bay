//! A command run on the GTK main loop.

use std::ffi::OsStr;
use std::time::Duration;

use gtk::gio;
use gtk::glib;

pub async fn text(argv: &[&str]) -> Result<String, String> {
    let owned: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    let proc = gio::Subprocess::newv(
        &owned,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
    )
    .map_err(|e| e.to_string())?;
    let (out, err) = proc
        .communicate_utf8_future(None)
        .await
        .map_err(|e| e.to_string())?;
    if proc.exit_status() != 0 {
        let err = err.unwrap_or_default();
        let err = err.trim();
        if err.is_empty() {
            return Err(format!("exit {}", proc.exit_status()));
        }
        return Err(err.to_owned());
    }
    Ok(out.unwrap_or_default().to_string())
}

/// Start a command and do not wait. Opening an installed app does not
/// have progress of its own.
pub fn spawn(argv: &[&str]) -> Result<(), String> {
    let owned: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    gio::Subprocess::newv(&owned, gio::SubprocessFlags::NONE).map_err(|e| e.to_string())?;
    Ok(())
}

pub struct CommandResult {
    pub code: i32,
    pub stderr: String,
}

enum Event {
    Line(String),
    Done(Result<CommandResult, String>),
}

/// Runs `argv` and calls `on_line` for each stdout chunk ended by a newline
/// or a carriage return. `on_tick` runs while the command is still going,
/// so the window can show that work started before the first percentage.
pub async fn stream(
    argv: &[String],
    mut on_line: impl FnMut(&str),
    mut on_tick: impl FnMut(Duration),
) -> Result<CommandResult, String> {
    let Some((program, args)) = argv.split_first() else {
        return Err("No command was given.".into());
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let program = program.clone();
    let args: Vec<String> = args.to_vec();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new(&program);
        command
            .args(&args)
            .env("LC_ALL", "C.UTF-8")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = tx.send(Event::Done(Err(format!("{program}: {err}"))));
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let tx_out = tx.clone();
        let out_handle = std::thread::spawn(move || {
            if let Some(stdout) = stdout {
                emit_lines(stdout, |line| {
                    let _ = tx_out.send(Event::Line(line));
                });
            }
        });
        let err_handle = std::thread::spawn(move || {
            let mut err = String::new();
            if let Some(mut stderr) = stderr {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
            }
            err
        });
        let status = child.wait();
        let _ = out_handle.join();
        let err = err_handle.join().unwrap_or_default();
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
        let _ = tx.send(Event::Done(Ok(CommandResult { code, stderr: err })));
    });
    let started = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(Event::Line(line)) => on_line(&line),
            Ok(Event::Done(result)) => return result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                on_tick(started.elapsed());
                glib::timeout_future(Duration::from_millis(40)).await;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("The command stopped before it reported a result.".into());
            }
        }
    }
}

fn emit_lines(mut from: impl std::io::Read, mut line: impl FnMut(String)) {
    let mut pending = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(n) = std::io::Read::read(&mut from, &mut chunk) {
        if n == 0 {
            break;
        }
        for &byte in &chunk[..n] {
            if byte == b'\n' || byte == b'\r' {
                if !pending.is_empty() {
                    line(String::from_utf8_lossy(&pending).into_owned());
                    pending.clear();
                }
            } else {
                pending.push(byte);
            }
        }
    }
    if !pending.is_empty() {
        line(String::from_utf8_lossy(&pending).into_owned());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_oauth::init())
    .plugin(tauri_plugin_google_auth::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

      // The main window is created here (instead of tauri.conf.json) so the
      // webview's popup handler can be allowed — Google sign-in uses a popup
      // window, which is blocked by default on every platform.
      #[cfg(not(mobile))]
      {
        use tauri::webview::NewWindowResponse;
        let _ = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
          .title("Chess Coach Utility")
          .theme(Some(tauri::Theme::Dark))
          .inner_size(1440.0, 900.0)
          .min_inner_size(1024.0, 700.0)
          .resizable(true)
          .fullscreen(false)
          .center()
          .maximized(true)
          .on_new_window(|_, _| NewWindowResponse::Allow)
          .build();
      }

      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
  download_and_launch_setup,
  device_id,
])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

/// Desktop in-app updater (ref/v4.0.2, exact port): downloads the setup
/// binary to the OS temp dir with progress events, then launches it so the
/// system installer takes over. The browser is never involved.
///
/// Progress is reported as `update-download-progress` (0–100) so the
/// frontend can drive the Download-button UI. Redirects (e.g. GitHub
/// release asset URLs) are followed automatically.
#[tauri::command]
async fn download_and_launch_setup<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    url: String,
) -> Result<String, String> {
    use tauri::Emitter;

    // Flat file name only — never let a URL smuggle in path separators.
    let raw_name = url
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let file_name = if raw_name.to_lowercase().ends_with(".exe")
        && !raw_name.contains(['/', '\\'])
        && raw_name.len() < 128
    {
        raw_name.to_string()
    } else {
        "CCU-Setup.exe".to_string()
    };

    let dir = std::env::temp_dir().join("CCU-Update");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not prepare download folder: {e}"))?;
    let dest = dir.join(&file_name);

    let resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total: Option<u64> = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok());
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&dest)
        .map_err(|e| format!("Could not write installer: {e}"))?;
    let mut buf = [0u8; 65536];
    let mut done: u64 = 0;
    loop {
        use std::io::Read;
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buf[..n])
            .map_err(|e| format!("Could not write installer: {e}"))?;
        done += n as u64;
        if let Some(t) = total {
            if t > 0 {
                let pct = (done.min(t) * 100 / t).min(100) as u8;
                let _ = app.emit("update-download-progress", pct);
            }
        }
    }
    drop(file);

    // Hand off to the system installer, detached: the app stays usable and
    // the user completes the update in the setup window, then relaunches.
    std::process::Command::new(&dest)
        .spawn()
        .map_err(|e| format!("Downloaded, but could not launch installer: {e}"))?;
    Ok(dest.to_string_lossy().into_owned())
}

/// Stable, per-machine identifier used to bind a licence to this device. The
/// webview hashes it before it is sent anywhere. It survives reinstalls, so
/// the owner can release and re-activate a seat when hardware is replaced.
#[tauri::command]
fn device_id() -> Result<String, String> {
  machine_uid::get().map_err(|e| e.to_string())
}

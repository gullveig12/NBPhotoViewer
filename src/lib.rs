pub mod engine;
pub mod export;
pub mod formats;
pub mod raw;
mod thumbnail_work;
mod storage;
pub use storage::data_dir;
use engine::{Collection, DeleteReport, Engine};
use std::sync::Arc;

#[tauri::command]
fn supported_formats() -> serde_json::Value {
    serde_json::json!({"images": formats::IMAGES, "raw": formats::RAWS})
}

#[tauri::command]
async fn select_sources(
    paths: Vec<String>,
    state: tauri::State<'_, Arc<Engine>>,
) -> Result<Collection, String> {
    let engine = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.select(paths))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn set_mark(
    id: String,
    marked: bool,
    state: tauri::State<'_, Arc<Engine>>,
) -> Result<(), String> {
    let engine = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.mark(&id, marked))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn trash_photo(id: String, state: tauri::State<'_, Arc<Engine>>) -> Result<(), String> {
    let engine = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.delete(&id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn trash_photos(
    ids: Vec<String>,
    state: tauri::State<'_, Arc<Engine>>,
) -> Result<DeleteReport, String> {
    let engine = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.delete_many(ids))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn copy_photo(
    id: String,
    prefer_raw: bool,
    job_id: String,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Engine>>,
    jobs: tauri::State<'_, Arc<export::ExportJobs>>,
) -> Result<export::ExportReport, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
    let engine = state.inner().clone();
    let jobs = jobs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _job = jobs.start(job_id)?;
        let path = engine.export_source(&id)?;
        let jpeg = export::jpeg(&path, prefer_raw)?;
        export::copy_jpeg(owner, &jpeg.bytes)?;
        let mut report = export::ExportReport {
            exported: 1,
            total: 1,
            ..Default::default()
        };
        if let Some(message) = jpeg.warning {
            report.warnings.push(export::ExportIssue {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                message,
            });
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn save_photo_jpeg(
    id: String,
    prefer_raw: bool,
    path: String,
    job_id: String,
    state: tauri::State<'_, Arc<Engine>>,
    jobs: tauri::State<'_, Arc<export::ExportJobs>>,
) -> Result<export::ExportReport, String> {
    let engine = state.inner().clone();
    let jobs = jobs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _job = jobs.start(job_id)?;
        let source = engine.export_source(&id)?;
        let jpeg = export::jpeg(&source, prefer_raw)?;
        let output = export::save_jpeg(std::path::Path::new(&path), &jpeg.bytes)?;
        let mut report = export::ExportReport {
            exported: 1,
            total: 1,
            files: vec![output.to_string_lossy().into_owned()],
            ..Default::default()
        };
        if let Some(message) = jpeg.warning {
            report.warnings.push(export::ExportIssue {
                name: source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                message,
            });
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn zip_export_default(session: tauri::State<'_, Arc<export::ZipSession>>) -> Result<String, String> {
    session.default_path()
}
#[tauri::command]
async fn export_zip(
    ids: Vec<String>,
    path: String,
    job_id: String,
    on_progress: tauri::ipc::Channel<export::ExportProgress>,
    state: tauri::State<'_, Arc<Engine>>,
    jobs: tauri::State<'_, Arc<export::ExportJobs>>,
    session: tauri::State<'_, Arc<export::ZipSession>>,
) -> Result<export::ExportReport, String> {
    let engine = state.inner().clone();
    let jobs = jobs.inner().clone();
    let session = session.inner().clone();
    // Register before dispatch so an immediate cancellation cannot be lost.
    let job = jobs.start(job_id)?;
    let cancel = job.cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        export::batch_named(
            &engine,
            ids,
            std::path::Path::new(&path),
            &session,
            &cancel,
            |p| {
                let _ = on_progress.send(p);
            },
            export::ZIP_LIMIT,
        )
    })
    .await
    .map_err(|e| e.to_string());
    drop(job);
    result
}
#[tauri::command]
async fn export_jpegs(
    ids: Vec<String>,
    directory: String,
    job_id: String,
    on_progress: tauri::ipc::Channel<export::ExportProgress>,
    state: tauri::State<'_, Arc<Engine>>,
    jobs: tauri::State<'_, Arc<export::ExportJobs>>,
) -> Result<export::ExportReport, String> {
    let engine = state.inner().clone();
    let jobs = jobs.inner().clone();
    let job = jobs.start(job_id)?;
    let cancel = job.cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        export::batch_jpegs(
            &engine,
            ids,
            std::path::Path::new(&directory),
            &cancel,
            |p| { let _ = on_progress.send(p); },
        )
    })
    .await
    .map_err(|e| e.to_string());
    drop(job);
    result
}
#[tauri::command]
fn cancel_export(job_id: String, jobs: tauri::State<'_, Arc<export::ExportJobs>>) {
    jobs.cancel(&job_id);
}
pub fn run() {
    let data = data_dir().expect("无法准备本地照片标记，请保留旧版数据目录后重试");
    let engine = Engine::new(data.clone()).expect("无法打开本地缓存");
    let media_engine = engine.clone();
    let mut context = tauri::generate_context!();
    let window_config = context.config().app.windows[0].clone();
    context.config_mut().app.windows[0].create = false;
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(engine)
        .manage(Arc::new(export::ExportJobs::default()))
        .manage(Arc::new(export::ZipSession::default()))
        .register_asynchronous_uri_scheme_protocol("nbphoto", move |_ctx, request, responder| {
            let engine = media_engine.clone();
            let path = request.uri().path().to_string();
            let origin = request
                .headers()
                .get("Origin")
                .and_then(|v| v.to_str().ok())
                .filter(|v| {
                    matches!(
                        *v,
                        "http://tauri.localhost"
                            | "https://tauri.localhost"
                            | "http://localhost:1420"
                            | "http://127.0.0.1:1420"
                    )
                })
                .unwrap_or("http://tauri.localhost")
                .to_string();
            tauri::async_runtime::spawn_blocking(move || {
                let (status, mime, bytes) = match engine::resource(&engine, &path) {
                    Ok(m) => (200, m.mime, m.bytes),
                    Err(e) => (404, "text/plain; charset=utf-8", e.into_bytes()),
                };
                let response = tauri::http::Response::builder()
                    .status(status)
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", origin)
                    .header("Cache-Control", "no-store")
                    .body(bytes)
                    .unwrap();
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            supported_formats,
            select_sources,
            set_mark,
            trash_photo,
            trash_photos,
            copy_photo,
            save_photo_jpeg,
            export_zip,
            zip_export_default,
            export_jpegs,
            cancel_export
        ])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::from_config(app, &window_config)?
                .data_directory(data.join("webview"))
                .build()?;
            Ok(())
        })
        .run(context)
        .expect("应用启动失败");
}

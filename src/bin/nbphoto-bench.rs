use nbphoto_viewer::{
    engine::{Engine, resource},
    formats, raw,
};
use std::{path::PathBuf, time::Instant};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "nbphoto-bench inspect <file> | bench <folder> | serve <folder> [port] | export <folder> <output-directory> [zip-limit]"
        );
        return;
    }
    if args[1] == "inspect" {
        let t = Instant::now();
        let path = PathBuf::from(&args[2]);
        let meta = if formats::is_raw(&path) {
            raw::load(&path, 0).map(|d| d.meta)
        } else {
            formats::metadata(&path)
        };
        match meta {
            Ok(d) => println!(
                "{}\n{}ms",
                serde_json::to_string_pretty(&d).unwrap(),
                t.elapsed().as_millis()
            ),
            Err(e) => eprintln!("{e}"),
        };
        return;
    }
    let data = std::env::var_os("NBPHOTOVIEWER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".qa/data"));
    let engine = Engine::new(data).unwrap();
    let collection = engine.select(vec![args[2].clone()]).unwrap();
    if args[1] == "export" {
        let output = PathBuf::from(args.get(3).expect("output directory is required"));
        let limit = args
            .get(4)
            .map(|v| v.parse().unwrap())
            .unwrap_or(nbphoto_viewer::export::ZIP_LIMIT);
        let report = nbphoto_viewer::export::batch(
            &engine,
            collection.photos.iter().map(|p| p.id.clone()).collect(),
            &output,
            &std::sync::atomic::AtomicBool::new(false),
            |p| eprintln!("{} / {} {} {}", p.completed, p.total, p.phase, p.name),
            limit,
        );
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return;
    }
    if args[1] == "serve" {
        let trace_requests = std::env::var_os("NBPHOTOVIEWER_TRACE_REQUESTS").is_some();
        let port = args.get(3).map(String::as_str).unwrap_or("1421");
        let server = tiny_http::Server::http(format!("127.0.0.1:{port}")).unwrap();
        println!(
            "Read-only QA backend at http://127.0.0.1:{port}; {} photos",
            collection.photos.len()
        );
        for request in server.incoming_requests() {
            let engine = engine.clone();
            std::thread::spawn(move || {
                let path = request.url().split('?').next().unwrap_or("");
                if trace_requests {
                    eprintln!("{} {path}", request.method());
                }
                let (status, mime, bytes) = if request.method() != &tiny_http::Method::Get {
                    (405, "text/plain", b"Read only".to_vec())
                } else if path == "/collection" {
                    (
                        200,
                        "application/json",
                        serde_json::to_vec(&*engine.collection.read().unwrap()).unwrap(),
                    )
                } else {
                    match resource(&engine, path) {
                        Ok(m) => (200, m.mime, m.bytes),
                        Err(e) => (404, "text/plain", e.into_bytes()),
                    }
                };
                let mut response = tiny_http::Response::from_data(bytes).with_status_code(status);
                for (key, value) in [
                    ("Content-Type", mime),
                    ("Access-Control-Allow-Origin", "http://127.0.0.1:1420"),
                    ("Cache-Control", "no-store"),
                ] {
                    response.add_header(tiny_http::Header::from_bytes(key, value).unwrap())
                }
                let _ = request.respond(response);
            });
        }
    } else {
        let mut results = vec![];
        for photo in collection.photos.iter().take(8) {
            let mut row = serde_json::json!({"file":photo.name});
            for kind in ["preview", "raw", "thumb"] {
                let t = Instant::now();
                match engine.media(&photo.id, kind) {
                    Ok(m) => {
                        row[kind] =
                            serde_json::json!({"ms":t.elapsed().as_millis(),"bytes":m.bytes.len()})
                    }
                    Err(e) => row[kind] = serde_json::json!({"error":e}),
                }
            }
            results.push(row)
        }
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
    }
}

# JPEG export validation · 0.5.0

2026-09-21, Windows x64.

- Rust release: 26 tests passed (7 export, 12 formats, 6 library, 1 scheduler). Node histogram: 6 passed. TypeScript passed; frontend and native release builds completed.
- Export tests inspect actual JPEG quantization tables (all ones, quality 100), 4:4:4 sampling, orientation, dimensions, EXIF exposure and ICC. Full RAW development without embedded preview produces 1936×1090. Transparent pixels become white.
- Save tests verify existing files and source JPEGs are not overwritten. Stale source identity is rejected. Batch tests cover selected-only output, duplicate IDs/names, invalid images, cancellation, strict size boundary (equal is rejected, one byte higher is accepted), and no orphan temporary parts on normal cancellation.
- Real mixed samples: NEF, CR3, ARW, RAF, DNG, JPG, PNG, TIFF, BMP, GIF, WebP and portrait PNG: 12/12 exported. A 35,000,000-byte injected test cap created four independent ZIPs of 26,385,054 / 19,126,492 / 26,431,709 / 20,313,671 bytes. Python zipfile verified every CRC; Pillow decoded all 12 JPEGs and independently checked quality tables and 4:4:4. Source SHA256 values were unchanged. Production cap is strictly below 1,000,000,000 bytes with the same writer.
- Browser interaction QA at 900×600: batch export is immediately left of delete, disabled with zero selection; marked selection/inversion work; progress/completion/cancellation recover controls. Detail context menu offers copy and JPEG save at 100% zoom; original photo/count/zoom stay unchanged. No horizontal overflow or console errors observed. Browser mode simulates filesystem and clipboard operations; real file output is checked through the native engine above.
- Native WIC clipboard payload tested without changing the user's clipboard: full oriented CF_DIB dimensions and pixel channels verified. JPEG and image/jpeg registration/publishing code compiled. Actual WeChat paste and native save-dialog interaction have not been exercised; no messages were sent to other apps.

JPEG quality 100 remains lossy. RAW uses a large camera preview when adequate, otherwise attempts full development. Unsupported RAW development may fall back to preview with an explicit warning. Tests use the small cap to exercise splitting; a >1 GB output set was not needed for the boundary test.

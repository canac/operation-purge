//! Stamps the embedded assets with a content hash, and embeds the exe icon
//! and version info when targeting Windows.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=res/icon.ico");
    println!("cargo:rerun-if-changed=assets");
    println!("cargo:rustc-env=ASSETS_HASH={:016x}", hash_tree(Path::new("assets")));

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("res/icon.ico")
            .compile()
            .expect("embed Windows resources");
    }
}

/// FNV-1a over every file's relative path and bytes, in sorted order.
fn hash_tree(root: &Path) -> u64 {
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    for path in files {
        let rel = path.strip_prefix(root).expect("under root");
        h.write(rel.to_string_lossy().replace('\\', "/").as_bytes());
        h.write(&[0]);
        h.write(&std::fs::read(&path).expect("read asset"));
    }
    h.0
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read assets dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

struct Fnv(u64);

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

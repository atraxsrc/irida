use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use tempfile::TempDir;

use irida_core::wallpaper::{WallpaperError, current_wallpaper_in, load_pixels, name_from_file};

const KEY_DIR: &str = "com.system76.CosmicBackground/v1";

/// A temp folder with empty `user` and `system` config folders.
struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Env {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("user")).unwrap();
        fs::create_dir_all(tmp.path().join("system")).unwrap();
        Env { tmp }
    }

    fn user(&self) -> PathBuf {
        self.tmp.path().join("user")
    }

    fn system(&self) -> PathBuf {
        self.tmp.path().join("system")
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    fn current(&self) -> Option<PathBuf> {
        current_wallpaper_in(&self.user(), &self.system())
    }
}

fn write_key(root: &Path, key: &str, text: &str) {
    let dir = root.join(KEY_DIR);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(key), text).unwrap();
}

/// COSMIC's shipped default entry, pointing at `path`.
fn entry(output: &str, path: &Path) -> String {
    format!(
        "(\n    output: {output:?},\n    source: Path({:?}),\n    filter_by_theme: #true,\n    rotation_frequency: 3600,\n    filter_method: Lanczos,\n    scaling_mode: Zoom,\n    sampling_method: Alphanumeric,\n)",
        path.to_str().unwrap()
    )
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"not really an image").unwrap();
}

/// Every path under `root`, files and folders.
fn listing(root: &Path) -> BTreeSet<PathBuf> {
    fn walk(dir: &Path, out: &mut BTreeSet<PathBuf>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            out.insert(p.clone());
            if p.is_dir() {
                walk(&p, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, &mut out);
    out
}

#[test]
fn current_from_all() {
    let env = Env::new();
    let w = env.path("w.png");
    touch(&w);
    write_key(&env.user(), "all", &entry("all", &w));
    assert_eq!(env.current(), Some(w));
}

#[test]
fn current_falls_back_to_system() {
    let env = Env::new();
    let w = env.path("w.png");
    touch(&w);
    write_key(&env.system(), "all", &entry("all", &w));
    assert_eq!(env.current(), Some(w));
}

#[test]
fn current_per_output() {
    let env = Env::new();
    let a = env.path("a.png");
    let b = env.path("b.png");
    touch(&a);
    touch(&b);
    write_key(&env.user(), "all", &entry("all", &a));
    write_key(&env.user(), "same-on-all", "false");
    write_key(&env.user(), "backgrounds", "[\"DP-1\"]");
    write_key(&env.user(), "output.DP-1", &entry("DP-1", &b));
    assert_eq!(env.current(), Some(b.clone()));

    // The same keys split between the user and the system folders.
    let env = Env::new();
    write_key(&env.system(), "all", &entry("all", &a));
    write_key(&env.system(), "same-on-all", "false");
    write_key(&env.user(), "backgrounds", "[\"DP-1\", \"HDMI-A-1\"]");
    write_key(&env.system(), "output.DP-1", &entry("DP-1", &b));
    assert_eq!(env.current(), Some(b.clone()));

    // same-on-all true (or missing) uses `all` even with outputs listed.
    let env = Env::new();
    write_key(&env.user(), "all", &entry("all", &a));
    write_key(&env.user(), "backgrounds", "[\"DP-1\"]");
    write_key(&env.user(), "output.DP-1", &entry("DP-1", &b));
    assert_eq!(env.current(), Some(a.clone()));
    write_key(&env.user(), "same-on-all", "true");
    assert_eq!(env.current(), Some(a.clone()));

    // An output named but with no entry anywhere falls back to `all`.
    let env = Env::new();
    write_key(&env.user(), "all", &entry("all", &a));
    write_key(&env.user(), "same-on-all", "false");
    write_key(&env.user(), "backgrounds", "[\"DP-1\"]");
    assert_eq!(env.current(), Some(a));
}

#[test]
fn current_folder_colour_missing() {
    let env = Env::new();
    let folder = env.path("walls");
    touch(&folder.join("b.PNG"));
    touch(&folder.join("a.txt"));
    touch(&folder.join("c.jpg"));
    fs::create_dir_all(folder.join("0.jpg")).unwrap();
    write_key(&env.user(), "all", &entry("all", &folder));
    let before = (listing(&env.user()), listing(&env.system()));
    assert_eq!(env.current(), Some(folder.join("b.PNG")));

    // A colour source.
    write_key(
        &env.user(),
        "all",
        "(\n    output: \"all\",\n    source: Color(Single((0.1, 0.1, 0.1))),\n    filter_by_theme: #true,\n    rotation_frequency: 3600,\n    filter_method: Lanczos,\n    scaling_mode: Zoom,\n    sampling_method: Alphanumeric,\n)",
    );
    assert_eq!(env.current(), None);

    // A gradient source.
    write_key(
        &env.user(),
        "all",
        "(output: \"all\", source: Color(Gradient((colors: [(0.1, 0.2, 0.3), (0.4, 0.5, 0.6)], radius: 270.0))))",
    );
    assert_eq!(env.current(), None);

    // A missing file.
    write_key(&env.user(), "all", &entry("all", &env.path("gone.png")));
    assert_eq!(env.current(), None);

    // A folder with no image in it.
    let empty = env.path("empty");
    touch(&empty.join("notes.txt"));
    write_key(&env.user(), "all", &entry("all", &empty));
    assert_eq!(env.current(), None);

    // Garbage in `all`, even with a good one in `system`.
    let w = env.path("w.png");
    touch(&w);
    write_key(&env.system(), "all", &entry("all", &w));
    write_key(&env.user(), "all", "this is not ron (");
    assert_eq!(env.current(), None);

    // An unparsable `same-on-all` gives None too.
    write_key(&env.user(), "all", &entry("all", &w));
    write_key(&env.user(), "same-on-all", "maybe");
    assert_eq!(env.current(), None);

    // Nothing at all.
    let bare = Env::new();
    assert_eq!(bare.current(), None);
    assert!(listing(&bare.user()).is_empty());
    assert!(listing(&bare.system()).is_empty());
    // Folders that do not exist are not created.
    let gone = bare.path("nowhere");
    assert_eq!(current_wallpaper_in(&gone, &gone), None);
    assert!(!gone.exists());

    let after = (listing(&env.user()), listing(&env.system()));
    let system_key = env.system().join(KEY_DIR);
    let mut want = before;
    want.1
        .insert(env.system().join("com.system76.CosmicBackground"));
    want.1.insert(system_key.clone());
    want.1.insert(system_key.join("all"));
    want.0.insert(env.user().join(KEY_DIR).join("same-on-all"));
    assert_eq!(after, want, "only the test's own writes are there");
}

#[test]
fn hash_true_inside_a_path_is_kept() {
    let env = Env::new();
    let w = env.path("#true #false.png");
    touch(&w);
    write_key(&env.user(), "all", &entry("all", &w));
    assert_eq!(env.current(), Some(w));
}

fn write_png(path: &Path, w: u32, h: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            (x * 7 % 256) as u8,
            (y * 13 % 256) as u8,
            ((x + y) % 256) as u8,
            255,
        ])
    });
    img.save(path).unwrap();
    img.into_raw()
}

#[test]
fn load_pixels_scales() {
    let tmp = TempDir::new().unwrap();
    let big = tmp.path().join("big.png");
    write_png(&big, 640, 320);
    assert_eq!(load_pixels(&big).unwrap().len(), 160 * 80 * 4);

    let tall = tmp.path().join("tall.png");
    write_png(&tall, 100, 400);
    assert_eq!(load_pixels(&tall).unwrap().len(), 40 * 160 * 4);

    let small = tmp.path().join("small.png");
    let raw = write_png(&small, 40, 25);
    assert_eq!(load_pixels(&small).unwrap(), raw, "unchanged");

    let edge = tmp.path().join("edge.png");
    let raw = write_png(&edge, 160, 90);
    assert_eq!(load_pixels(&edge).unwrap(), raw, "160 px is used as is");

    let text = tmp.path().join("notes.txt");
    fs::write(&text, "hello").unwrap();
    assert!(matches!(load_pixels(&text), Err(WallpaperError::NotImage(p)) if p == text));

    // A text file named like an image is still not an image.
    let fake = tmp.path().join("fake.png");
    fs::write(&fake, "hello").unwrap();
    let err = load_pixels(&fake).unwrap_err();
    assert!(matches!(&err, WallpaperError::NotImage(p) if *p == fake));
    assert!(
        err.to_string().contains("could not be read as an image"),
        "{err}"
    );

    let gone = tmp.path().join("gone.png");
    let err = load_pixels(&gone).unwrap_err();
    assert!(
        matches!(&err, WallpaperError::Io(p, _) if *p == gone),
        "{err:?}"
    );
    assert!(err.to_string().contains("gone.png"), "{err}");
}

#[test]
fn name_from_file_cases() {
    for (file, want) in [
        ("aurora-forest.jpg", "Aurora Forest"),
        ("IMG_0042.png", "Img 0042"),
        ("/some/folder/mOuNTAIN lake.webp", "Mountain Lake"),
        ("one-two-three-four-five.png", "One Two Three Four"),
        ("___.jpg", "Wallpaper"),
        ("2024.jpg", "Wallpaper"),
        ("20240105_123456.jpg", "Wallpaper"),
        ("caf\u{e9}-noir.png", "Caf Noir"),
    ] {
        assert_eq!(name_from_file(Path::new(file)), want, "{file}");
    }
}

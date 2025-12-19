use std::env;
use std::path::Path;
use std::process::Command;

fn build_ffmpeg(dist_dir: &Path, enable_libnpp: bool) {
    if dist_dir.exists() {
        return;
    }

    let _ = Command::new("bash")
        .args([
            "-c",
            "for f in clean.sh build.sh download.sh x264.sh ffmpeg.sh nv-codec-headers.sh libva.sh clean_all.sh; do \
                test -f \"$f\" && sed -i 's/\\r$//' \"$f\"; \
            done",
        ])
        .current_dir("deps")
        .status();

    Command::new("bash")
        .arg(Path::new("clean.sh"))
        .current_dir("deps")
        .status()
        .expect("Failed to clean ffmpeg build!");

    if !Command::new("bash")
        .arg(Path::new("build.sh"))
        .current_dir("deps")
        .env("DIST", dist_dir)
        .env("ENABLE_LIBNPP", if enable_libnpp { "y" } else { "n" })
        .status()
        .expect("Failed to run bash!")
        .success()
    {
        println!("cargo:warning=Failed to build ffmpeg!");
        std::process::exit(1);
    }
}

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let dist_dir = Path::new("deps")
        .canonicalize()
        .unwrap()
        .join(format!("dist_{target_os}"));

    let enable_libnpp = env::var("I_AM_BUILDING_THIS_AT_HOME_AND_WANT_LIBNPP")
        .is_ok_and(|v| ["y", "yes", "true", "1"].contains(&v.to_lowercase().as_str()));

    if env::var("CARGO_FEATURE_FFMPEG_SYSTEM").is_err() {
        build_ffmpeg(&dist_dir, enable_libnpp);
    }

    println!("cargo:rerun-if-changed=ts/lib.ts");

    let js_needs_update = || -> Result<bool, Box<dyn std::error::Error>> {
        Ok(Path::new("ts/lib.ts").metadata()?.modified()?
            > Path::new("www/static/lib.js").metadata()?.modified()?)
    }()
    .unwrap_or(true);

    if js_needs_update {
        #[cfg(not(target_os = "windows"))]
        let status = Command::new("tsc").status();

        #[cfg(target_os = "windows")]
        let status = Command::new("cmd").args(["/C", "tsc"]).status();

        let status = match status {
            Ok(status) => status,
            Err(err) => {
                println!("cargo:warning=Failed to call tsc: {err}");
                std::process::exit(1);
            }
        };

        if !status.success() {
            match status.code() {
                Some(code) => println!("cargo:warning=tsc failed with exitcode: {code}"),
                None => println!("cargo:warning=tsc terminated by signal."),
            };
            std::process::exit(2);
        }
    }

    println!("cargo:rerun-if-changed=lib/encode_video.c");
    let mut cc_video = cc::Build::new();
    cc_video.file("lib/encode_video.c");
    cc_video.include(dist_dir.join("include"));
    if ["linux", "windows"].contains(&target_os.as_str()) {
        cc_video.define("HAS_NVENC", None);
    }
    if target_os == "linux" {
        cc_video.define("HAS_VAAPI", None);
    }
    if target_os == "macos" {
        cc_video.define("HAS_VIDEOTOOLBOX", None);
    }
    if target_os == "windows" {
        cc_video.define("HAS_MEDIAFOUNDATION", None);
    }
    if enable_libnpp {
        cc_video.define("HAS_LIBNPP", None);
    }
    cc_video.compile("video");

    println!("cargo:rerun-if-changed=lib/error.h");
    println!("cargo:rerun-if-changed=lib/error.c");
    println!("cargo:rerun-if-changed=lib/log.h");
    println!("cargo:rerun-if-changed=lib/log.c");
    cc::Build::new().file("lib/error.c").compile("error");
    cc::Build::new().file("lib/log.c").compile("log");

    let ffmpeg_link_kind =
        // https://github.com/rust-lang/rust/pull/72785
        // https://users.rust-lang.org/t/linking-on-windows-without-wholearchive/49846/3
        if cfg!(target_os = "windows") ||
            env::var("CARGO_FEATURE_FFMPEG_SYSTEM").is_ok() {
            "dylib"
        } else {
            "static"
        };
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=avdevice");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=avformat");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=avfilter");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=avcodec");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=swresample");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=swscale");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=avutil");
    println!("cargo:rustc-link-lib={ffmpeg_link_kind}=x264");
    if enable_libnpp {
        if let Ok(lib_paths) = env::var("LIBRARY_PATH") {
            for lib_path in lib_paths.split(':') {
                println!("cargo:rustc-link-search={lib_path}");
            }
        }
        println!("cargo:rustc-link-lib=dylib=nppig");
        println!("cargo:rustc-link-lib=dylib=nppicc");
        println!("cargo:rustc-link-lib=dylib=nppc");
        println!("cargo:rustc-link-lib=dylib=nppidei");
        println!("cargo:rustc-link-lib=dylib=nppif");
    }
    if env::var("CARGO_FEATURE_FFMPEG_SYSTEM").is_err() {
        println!(
            "cargo:rustc-link-search={}",
            dist_dir.join("lib").to_string_lossy()
        );
    }

    if target_os == "linux" {
        linux();
    }

    if target_os == "macos" {
        println!("cargo:rustc-link-lib=framework=VideoToolbox");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
    }

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=mfplat");
        println!("cargo:rustc-link-lib=dylib=mfuuid");
        println!("cargo:rustc-link-lib=dylib=ole32");
        println!("cargo:rustc-link-lib=dylib=strmiids");
        println!("cargo:rustc-link-lib=dylib=vfw32");
        println!("cargo:rustc-link-lib=dylib=shlwapi");
        println!("cargo:rustc-link-lib=dylib=bcrypt");
    }
}

fn linux() {
    println!("cargo:rerun-if-changed=lib/linux/uniput.c");
    println!("cargo:rerun-if-changed=lib/linux/xcapture.c");
    println!("cargo:rerun-if-changed=lib/linux/xhelper.c");
    println!("cargo:rerun-if-changed=lib/linux/xhelper.h");

    cc::Build::new()
        .file("lib/linux/uinput.c")
        .file("lib/linux/xcapture.c")
        .file("lib/linux/xhelper.c")
        .compile("linux");

    println!("cargo:rustc-link-lib=X11");
    println!("cargo:rustc-link-lib=Xext");
    println!("cargo:rustc-link-lib=Xrandr");
    println!("cargo:rustc-link-lib=Xfixes");
    println!("cargo:rustc-link-lib=Xcomposite");
    println!("cargo:rustc-link-lib=Xi");
    let va_link_kind = if env::var("CARGO_FEATURE_VA_STATIC").is_ok() {
        "static"
    } else {
        "dylib"
    };
    println!("cargo:rustc-link-lib={va_link_kind}=va");
    println!("cargo:rustc-link-lib={va_link_kind}=va-drm");
    println!("cargo:rustc-link-lib={va_link_kind}=va-x11");
    println!("cargo:rustc-link-lib=drm");
    println!("cargo:rustc-link-lib=xcb-dri3");
    println!("cargo:rustc-link-lib=X11-xcb");
    println!("cargo:rustc-link-lib=xcb");
}

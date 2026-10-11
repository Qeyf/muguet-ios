// muguet - build.rs
// Constantin Clerc (c22dev) - 0.1

use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(game_code)");
    println!("cargo:rerun-if-env-changed=MUGUET_GAME_CODE");
    println!("cargo:rerun-if-env-changed=MUGUET_GAME_BUILD_ID");
    println!("cargo:rerun-if-env-changed=MUGUET_GAME_WASM_SHA256");
    println!("cargo:rerun-if-env-changed=MUGUET_IOS_HEAP_GB");
    if let Ok(path) = std::env::var("MUGUET_GAME_CODE") {
        assert!(std::path::Path::new(&path).is_file(), "MUGUET_GAME_CODE must name an existing precompiled module");
        println!("cargo:rustc-cfg=game_code");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("ios") {
        return;
    }
    println!("cargo:rustc-link-arg-bins=-Wl,-segprot,__GAME,rx,rx");

    println!("cargo:rerun-if-changed=ios/Launcher.swift");
    let out = std::env::var("OUT_DIR").unwrap();
    let xcrun = |args: &[&str]| {
        let o = Command::new("xcrun").args(args).output().expect("xcrun");
        assert!(o.status.success(), "xcrun {} failed: {}", args.join(" "), String::from_utf8_lossy(&o.stderr));
        let path = String::from_utf8(o.stdout).unwrap().trim().to_string();
        assert!(!path.is_empty(), "xcrun {} returned no path", args.join(" "));
        path
    };
    let simulator = std::env::var("TARGET").unwrap().ends_with("-sim");
    let (sdk_name, swift_target) = if simulator {
        ("iphonesimulator", "arm64-apple-ios15.0-simulator")
    } else {
        ("iphoneos", "arm64-apple-ios15.0")
    };
    let sdk = xcrun(&["--sdk", sdk_name, "--show-sdk-path"]);
    let swiftc = xcrun(&["--sdk", sdk_name, "-f", "swiftc"]);
    let status = Command::new(&swiftc)
        .args(["-parse-as-library", "-emit-library", "-static", "-O", "-module-name", "MuguetLauncher"])
        .args(["-target", swift_target, "-sdk", &sdk])
        .arg("ios/Launcher.swift")
        .arg("-o")
        .arg(format!("{out}/libmuguetlauncher.a"))
        .status()
        .expect("swiftc");
    assert!(status.success(), "swiftc failed");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=muguetlauncher");

    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    let toolchain = std::path::Path::new(&swiftc).parent().unwrap().parent().unwrap().join("lib/swift").join(sdk_name);
    println!("cargo:rustc-link-search=native={}", toolchain.display());
}

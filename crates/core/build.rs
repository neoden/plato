use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    // Cross-compiling for Kobo.
    if target == "arm-unknown-linux-gnueabihf" {
        println!("cargo:rustc-env=PKG_CONFIG_ALLOW_CROSS=1");
        println!("cargo:rustc-link-search=target/mupdf_wrapper/Kobo");
        println!("cargo:rustc-link-search=libs");
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=bz2");
        println!("cargo:rustc-link-lib=jpeg");
        println!("cargo:rustc-link-lib=png16");
        println!("cargo:rustc-link-lib=gumbo");
        println!("cargo:rustc-link-lib=openjp2");
        println!("cargo:rustc-link-lib=jbig2dec");
    // Handle the Linux and macOS platforms.
    } else {
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
        match target_os.as_ref() {
            "linux" => {
                println!("cargo:rustc-link-search=target/mupdf_wrapper/Linux");
                // Host build of MuPDF 1.27.0 from thirdparty/ source: a single,
                // self-contained libmupdf.so (bundled third-party libraries).
                println!("cargo:rustc-link-search=native=thirdparty/mupdf/build/shared-release");
                println!("cargo:rustc-link-lib=dylib=stdc++");
            },
            "macos" => {
                println!("cargo:rustc-link-search=target/mupdf_wrapper/Darwin");
                println!("cargo:rustc-link-lib=dylib=c++");
                println!("cargo:rustc-link-lib=mupdf-third");
                println!("cargo:rustc-link-lib=z");
                println!("cargo:rustc-link-lib=bz2");
                println!("cargo:rustc-link-lib=jpeg");
                println!("cargo:rustc-link-lib=png16");
                println!("cargo:rustc-link-lib=gumbo");
                println!("cargo:rustc-link-lib=openjp2");
                println!("cargo:rustc-link-lib=jbig2dec");
            },
            _ => panic!("Unsupported platform: {}.", target_os),
        }
    }
}

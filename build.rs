#![allow(clippy::uninlined_format_args)]
// When both `build-tesseract` and `use-system-tesseract` are enabled, the
// bundled-build module is compiled (it hosts shared helpers) but its build
// entry point is not called — silence the resulting dead-code warnings.
#![cfg_attr(
    all(feature = "build-tesseract", feature = "use-system-tesseract"),
    allow(dead_code)
)]

#[cfg(feature = "build-tesseract")]
mod build_tesseract {
    use cmake::Config;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    // Use specific release versions for stability.
    // NOTE: when bumping these, also update the Windows library name variants
    // in build_and_normalize() (leptonica-<ver>.lib and tesseract<MAJ><MIN>.lib).
    const LEPTONICA_URL: &str =
        "https://github.com/DanBloomberg/leptonica/archive/refs/tags/1.87.0.zip";
    const TESSERACT_URL: &str =
        "https://github.com/tesseract-ocr/tesseract/archive/refs/tags/5.5.2.zip";

    pub fn get_user_data_dir() -> PathBuf {
        if cfg!(target_os = "macos") {
            let home_dir = env::var("HOME").unwrap_or_else(|_| {
                env::var("USER")
                    .map(|user| format!("/Users/{}", user))
                    .expect("Neither HOME nor USER environment variable set")
            });
            PathBuf::from(home_dir)
                .join("Library")
                .join("Application Support")
                .join("tesseract-rs")
        } else if cfg!(target_os = "linux") {
            let home_dir = env::var("HOME").unwrap_or_else(|_| {
                env::var("USER")
                    .map(|user| format!("/home/{}", user))
                    .expect("Neither HOME nor USER environment variable set")
            });
            PathBuf::from(home_dir).join(".tesseract-rs")
        } else if cfg!(target_os = "freebsd") {
            let home_dir = env::var("HOME").unwrap_or_else(|_| {
                env::var("USER")
                    .map(|user| format!("/home/{}", user))
                    .expect("Neither HOME nor USER environment variable set")
            });
            PathBuf::from(home_dir).join(".tesseract-rs")
        } else if cfg!(target_os = "windows") {
            env::var("APPDATA")
                .or_else(|_| env::var("USERPROFILE").map(|p| format!("{}\\AppData\\Roaming", p)))
                .map(PathBuf::from)
                .expect("Neither APPDATA nor USERPROFILE environment variable set")
                .join("tesseract-rs")
        } else {
            panic!("Unsupported operating system");
        }
    }

    pub fn build() {
        let user_data_dir = get_user_data_dir();
        fs::create_dir_all(&user_data_dir).expect("Failed to create user data directory");

        let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
        let third_party_dir = out_dir.join("third_party");

        let leptonica_src_dir = if third_party_dir.join("leptonica").exists() {
            println!("cargo:warning=Using existing leptonica source");
            third_party_dir.join("leptonica")
        } else {
            fs::create_dir_all(&third_party_dir).expect("Failed to create third_party directory");
            download_and_extract(&third_party_dir, LEPTONICA_URL, "leptonica")
        };

        let tesseract_src_dir = if third_party_dir.join("tesseract").exists() {
            println!("cargo:warning=Using existing tesseract source");
            third_party_dir.join("tesseract")
        } else {
            fs::create_dir_all(&third_party_dir).expect("Failed to create third_party directory");
            download_and_extract(&third_party_dir, TESSERACT_URL, "tesseract")
        };

        let (cmake_cxx_flags, additional_defines) = get_os_specific_config();

        let leptonica_install_dir = out_dir.join("leptonica");
        build_and_normalize("leptonica", &leptonica_install_dir, || {
            let mut leptonica_config = Config::new(&leptonica_src_dir);
            leptonica_config.out_dir(&leptonica_install_dir);

            // Configure build tools
            if cfg!(target_os = "windows") {
                // Use NMake on Windows for better compatibility
                if let Ok(_vs_install_dir) = env::var("VSINSTALLDIR") {
                    leptonica_config.generator("NMake Makefiles");
                }
            }

            // Only use sccache if not in CI
            if env::var("CI").is_err() && env::var("RUSTC_WRAPPER").unwrap_or_default() == "sccache"
            {
                leptonica_config
                    .env("CC", "sccache cc")
                    .env("CXX", "sccache c++");
            }
            leptonica_config
                .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
                .define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW")
                .profile("Release")
                .define("BUILD_PROG", "OFF")
                .define("BUILD_SHARED_LIBS", "OFF")
                .define("ENABLE_ZLIB", "OFF")
                .define("ENABLE_PNG", "OFF")
                .define("ENABLE_JPEG", "OFF")
                .define("ENABLE_TIFF", "OFF")
                .define("ENABLE_WEBP", "OFF")
                .define("ENABLE_OPENJPEG", "OFF")
                .define("ENABLE_GIF", "OFF")
                .define("CMAKE_CXX_FLAGS", &cmake_cxx_flags)
                .define("SW_BUILD", "OFF")
                .define("HAVE_LIBZ", "0")
                .define("ENABLE_LTO", "OFF");

            for (key, value) in &additional_defines {
                leptonica_config.define(key, value);
            }

            leptonica_config.build();
        });

        let leptonica_include_dir = leptonica_install_dir.join("include");
        let leptonica_lib_dir = leptonica_install_dir.join("lib");
        let tesseract_install_dir = out_dir.join("tesseract");
        let tessdata_prefix = user_data_dir.join("tessdata");

        build_and_normalize("tesseract", &tesseract_install_dir, || {
            let mut tesseract_config = Config::new(&tesseract_src_dir);
            tesseract_config.out_dir(&tesseract_install_dir);
            // Configure build tools
            if cfg!(target_os = "windows") {
                // Use NMake on Windows for better compatibility
                if let Ok(_vs_install_dir) = env::var("VSINSTALLDIR") {
                    tesseract_config.generator("NMake Makefiles");
                }
            }

            // Only use sccache if not in CI
            if env::var("CI").is_err() && env::var("RUSTC_WRAPPER").unwrap_or_default() == "sccache"
            {
                tesseract_config
                    .env("CC", "sccache cc")
                    .env("CXX", "sccache c++");
            }
            tesseract_config
                .define("CMAKE_POLICY_VERSION_MINIMUM", "3.5")
                .profile("Release")
                .define("BUILD_TRAINING_TOOLS", "OFF")
                .define("BUILD_SHARED_LIBS", "OFF")
                .define("DISABLE_ARCHIVE", "ON")
                .define("DISABLE_CURL", "ON")
                .define("DISABLE_OPENCL", "ON")
                .define("Leptonica_DIR", &leptonica_install_dir)
                .define("LEPTONICA_INCLUDE_DIR", &leptonica_include_dir)
                .define("LEPTONICA_LIBRARY", &leptonica_lib_dir)
                .define("CMAKE_PREFIX_PATH", &leptonica_install_dir)
                .define("TESSDATA_PREFIX", &tessdata_prefix)
                .define("DISABLE_TIFF", "ON")
                .define("DISABLE_PNG", "ON")
                .define("DISABLE_JPEG", "ON")
                .define("DISABLE_WEBP", "ON")
                .define("DISABLE_OPENJPEG", "ON")
                .define("DISABLE_ZLIB", "ON")
                .define("DISABLE_LIBXML2", "ON")
                .define("DISABLE_LIBICU", "ON")
                .define("DISABLE_LZMA", "ON")
                .define("DISABLE_GIF", "ON")
                .define("DISABLE_DEBUG_MESSAGES", "ON")
                .define("debug_file", "/dev/null")
                .define("HAVE_LIBARCHIVE", "OFF")
                .define("HAVE_LIBCURL", "OFF")
                .define("HAVE_TIFFIO_H", "OFF")
                .define("GRAPHICS_DISABLED", "ON")
                .define("DISABLED_LEGACY_ENGINE", "OFF")
                .define("USE_OPENCL", "OFF")
                .define(
                    "OPENMP_BUILD",
                    if cfg!(feature = "openmp") {
                        "ON"
                    } else {
                        "OFF"
                    },
                )
                .define("BUILD_TESTS", "OFF")
                .define("ENABLE_LTO", "OFF")
                .define("BUILD_PROG", "OFF")
                .define("SW_BUILD", "OFF")
                .define("LEPT_TIFF_RESULT", "FALSE")
                .define("INSTALL_CONFIGS", "ON")
                .define("USE_SYSTEM_ICU", "ON")
                .define("CMAKE_CXX_FLAGS", &cmake_cxx_flags);

            for (key, value) in &additional_defines {
                tesseract_config.define(key, value);
            }

            tesseract_config.build();
        });

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_FEATURE");
        emit_link_directives(&leptonica_install_dir, &tesseract_install_dir);

        println!("cargo:warning=Tessdata dir: {:?}", tessdata_prefix);

        download_tessdata(&user_data_dir);
    }

    fn get_os_specific_config() -> (String, Vec<(String, String)>) {
        let mut cmake_cxx_flags = String::new();
        let mut additional_defines = Vec::new();

        if cfg!(target_os = "macos") {
            cmake_cxx_flags.push_str("-stdlib=libc++ ");
            cmake_cxx_flags.push_str("-std=c++17 ");
            // Pin the deployment target to 10.15, the minimum macOS version
            // that provides std::filesystem (used by Tesseract 5.x
            // baseapi.cpp). Because CMAKE_CXX_FLAGS is set explicitly below,
            // the cmake crate skips injecting the cc crate's
            // -mmacosx-version-min into the C++ flags (it only does so for
            // C/ASM). Without an explicit deployment target, Xcode 26+
            // runners derive a default below 10.15 and the build fails
            // (issue #32). CMAKE_OSX_DEPLOYMENT_TARGET is the authoritative
            // cmake-level setting; the flag in CMAKE_CXX_FLAGS is redundant
            // but keeps the two consistent.
            cmake_cxx_flags.push_str("-mmacosx-version-min=10.15 ");
            additional_defines.push((
                "CMAKE_OSX_DEPLOYMENT_TARGET".to_string(),
                "10.15".to_string(),
            ));
        } else if cfg!(target_os = "linux") {
            cmake_cxx_flags.push_str("-std=c++17 ");
            // Check if we're on a system using clang
            if linux_uses_clang() {
                cmake_cxx_flags.push_str("-stdlib=libc++ ");
                additional_defines.push(("CMAKE_CXX_COMPILER".to_string(), "clang++".to_string()));
            } else {
                // Assume GCC
                additional_defines.push(("CMAKE_CXX_COMPILER".to_string(), "g++".to_string()));
            }
        } else if cfg!(target_os = "freebsd") {
            cmake_cxx_flags.push_str("-std=c++17 ");
            // FreeBSD typically uses clang by default
            cmake_cxx_flags.push_str("-stdlib=libc++ ");
            additional_defines.push(("CMAKE_CXX_COMPILER".to_string(), "clang++".to_string()));
        } else if cfg!(target_os = "windows") {
            // Windows-specific MSVC flags
            cmake_cxx_flags.push_str("/EHsc /MP /std:c++17 ");
            additional_defines.push(("CMAKE_CXX_FLAGS_RELEASE".to_string(), "/O2".to_string()));
            additional_defines.push((
                "CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS".to_string(),
                "ON".to_string(),
            ));
            if cfg!(target_env = "msvc") {
                let runtime_library = if target_uses_static_crt() {
                    "MultiThreaded"
                } else {
                    "MultiThreadedDLL"
                };
                // this requires CMP0091=NEW, which is default for tesseract and set above for leptonica
                additional_defines.push((
                    "CMAKE_MSVC_RUNTIME_LIBRARY".to_string(),
                    runtime_library.to_string(),
                ));
            }
        }

        // Common flags and defines for all platforms
        cmake_cxx_flags.push_str("-DUSE_STD_NAMESPACE ");
        // Keep recognition images in memory without PNG encoding/decoding.
        cmake_cxx_flags.push_str("-DTESSERACT_IMAGEDATA_AS_PIX ");
        // Debug caption fonts require TIFF support, which is disabled above.
        cmake_cxx_flags.push_str("-DTESSERACT_DISABLE_DEBUG_FONTS ");
        additional_defines.push((
            "CMAKE_POSITION_INDEPENDENT_CODE".to_string(),
            "ON".to_string(),
        ));

        (cmake_cxx_flags, additional_defines)
    }

    fn target_uses_static_crt() -> bool {
        env::var("CARGO_CFG_TARGET_FEATURE")
            .unwrap_or_default()
            .split(',')
            .any(|feature| feature == "crt-static")
    }

    fn linux_uses_clang() -> bool {
        cfg!(target_env = "musl")
            || env::var("CC")
                .map(|cc| cc.contains("clang"))
                .unwrap_or(false)
    }

    #[derive(Clone, Copy)]
    enum OpenMpRuntime {
        Llvm,
        Gnu,
    }

    impl OpenMpRuntime {
        fn link_name(self) -> RustcLinkName {
            match self {
                Self::Llvm => RustcLinkName::known("omp"),
                Self::Gnu => RustcLinkName::known("gomp"),
            }
        }
    }

    struct RustcLinkName(String);

    impl RustcLinkName {
        fn known(name: &str) -> Self {
            Self(name.to_string())
        }

        fn as_str(&self) -> &str {
            &self.0
        }
    }

    fn expected_openmp_runtime() -> Option<OpenMpRuntime> {
        if !cfg!(feature = "openmp") {
            None
        } else if cfg!(any(target_os = "macos", target_os = "freebsd"))
            || (cfg!(target_os = "linux") && linux_uses_clang())
        {
            Some(OpenMpRuntime::Llvm)
        } else if cfg!(target_os = "linux") {
            Some(OpenMpRuntime::Gnu)
        } else {
            //Windows openmp runtime is linked automatically, nothing to do for us
            None
        }
    }

    fn emit_link_directives(leptonica_install_dir: &Path, tesseract_install_dir: &Path) {
        println!(
            "cargo:rustc-link-search=native={}",
            leptonica_install_dir.join("lib").display()
        );
        println!("cargo:rustc-link-lib=static=leptonica");
        println!(
            "cargo:rustc-link-search=native={}",
            tesseract_install_dir.join("lib").display()
        );
        println!("cargo:rustc-link-lib=static=tesseract");

        if cfg!(target_os = "macos") {
            println!("cargo:rustc-link-lib=c++");
        } else if cfg!(target_os = "linux") {
            if linux_uses_clang() {
                println!("cargo:rustc-link-lib=c++");
            } else {
                println!("cargo:rustc-link-lib=stdc++");
            }
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=dl");
        } else if cfg!(target_os = "freebsd") {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=m");
        } else if cfg!(target_os = "windows") {
            // Additional linker flags are generally not required for Windows,
            // as MSVC automatically links the necessary libraries.
            // However, for some special cases, additions can be made as follows:
            // println!("cargo:rustc-link-lib=user32");
            // println!("cargo:rustc-link-lib=gdi32");
        }

        if let Some(expected_runtime) = expected_openmp_runtime() {
            let (search_dir, runtime_link_name) =
                openmp_link_info(tesseract_install_dir, expected_runtime);
            if let Some(search_dir) = search_dir {
                println!("cargo:rustc-link-search=native={}", search_dir.display());
            }
            println!("cargo:rustc-link-lib={}", runtime_link_name.as_str());
        }

        println!(
            "cargo:rustc-link-search=native={}",
            env::var("OUT_DIR").unwrap()
        );
    }

    fn openmp_link_info(
        tesseract_install_dir: &Path,
        expected_runtime: OpenMpRuntime,
    ) -> (Option<PathBuf>, RustcLinkName) {
        let library_path = find_openmp_library_in_cmake_cache(tesseract_install_dir);

        if let Some(library_path) = library_path {
            let runtime =
                library_link_name(&library_path).unwrap_or_else(|| expected_runtime.link_name());
            return (library_path.parent().map(Path::to_path_buf), runtime);
        }

        (None, expected_runtime.link_name())
    }

    fn find_openmp_library_in_cmake_cache(tesseract_install_dir: &Path) -> Option<PathBuf> {
        let cmake_cache = tesseract_install_dir.join("build").join("CMakeCache.txt");
        let contents = fs::read_to_string(cmake_cache).ok()?;
        let library_names = contents
            .lines()
            .find_map(|line| line.strip_prefix("OpenMP_CXX_LIB_NAMES:STRING="))?;

        library_names
            .split(';')
            // OpenMP_CXX_LIB_NAMES may also contain dependencies such as pthread.
            // Select the runtime-like entry: commonly gomp, libomp, or iomp5.
            .filter(|name| name.contains("omp"))
            .find_map(|name| {
                let key = format!("OpenMP_{name}_LIBRARY:FILEPATH=");
                contents.lines().find_map(|line| {
                    line.strip_prefix(&key)
                        .filter(|value| !value.ends_with("-NOTFOUND"))
                        .map(PathBuf::from)
                })
            })
    }

    /// Converts a library filename into the name expected by `rustc-link-lib`
    /// by removing an optional `lib` prefix and a recognized library suffix.
    ///
    /// For example, `libiomp5.so` becomes `iomp5`. CMake may select runtimes
    /// other than the usual `omp` and `gomp`.
    fn library_link_name(library_path: &Path) -> Option<RustcLinkName> {
        let file_name = library_path.file_name()?.to_str()?;
        let name = file_name.strip_prefix("lib").unwrap_or(file_name);
        let suffix = [".so", ".dylib", ".a", ".lib"]
            .iter()
            .filter_map(|suffix| name.find(suffix))
            .min()
            .unwrap_or(name.len());
        Some(RustcLinkName(name[..suffix].to_string()))
    }

    fn download_and_extract(target_dir: &Path, url: &str, name: &str) -> PathBuf {
        use reqwest::blocking::Client;
        use zip::ZipArchive;

        fs::create_dir_all(target_dir).expect("Failed to create target directory");

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        println!("cargo:warning=Downloading {} from {}", name, url);
        let mut response = client.get(url).send().expect("Failed to download archive");

        if !response.status().is_success() {
            panic!("Failed to download {}: HTTP {}", name, response.status());
        }

        let mut content = Vec::new();
        response
            .copy_to(&mut content)
            .expect("Failed to read archive content");

        println!(
            "cargo:warning=Downloaded {} bytes for {}",
            content.len(),
            name
        );

        let temp_file = target_dir.join(format!("{}.zip", name));
        fs::write(&temp_file, content).expect("Failed to write archive to file");

        let extract_dir = target_dir.join(name);
        if extract_dir.exists() {
            fs::remove_dir_all(&extract_dir).expect("Failed to remove existing directory");
        }
        fs::create_dir_all(&extract_dir).expect("Failed to create extraction directory");

        let mut archive = ZipArchive::new(fs::File::open(&temp_file).unwrap()).unwrap();

        // Extract files, ignoring the top-level directory
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let file_path = file.mangled_name();
            let file_path = file_path.to_str().unwrap();

            // Skip the top-level directory
            let path = Path::new(file_path);
            let path = path
                .strip_prefix(path.components().next().unwrap())
                .unwrap();

            if path.as_os_str().is_empty() {
                continue;
            }

            let target_path = extract_dir.join(path);

            if file.is_dir() {
                fs::create_dir_all(target_path).unwrap();
            } else {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                let mut outfile = fs::File::create(target_path).unwrap();
                std::io::copy(&mut file, &mut outfile).unwrap();
            }
        }

        fs::remove_file(temp_file).expect("Failed to remove temporary zip file");

        extract_dir
    }

    pub(crate) fn download_tessdata(project_dir: &Path) {
        let tessdata_dir = project_dir.join("tessdata");
        fs::create_dir_all(&tessdata_dir).expect("Failed to create Tessdata directory");

        let languages = ["eng", "tur"];
        let base_url = "https://github.com/tesseract-ocr/tessdata_best/raw/main/";
        let client = reqwest::blocking::Client::new();

        for lang in &languages {
            let filename = format!("{}.traineddata", lang);
            let file_path = tessdata_dir.join(&filename);

            if !file_path.exists() {
                let url = format!("{}{}", base_url, filename);
                let response = client
                    .get(&url)
                    .send()
                    .expect("Failed to download Tessdata");
                let mut dest = fs::File::create(&file_path).expect("Failed to create file");
                std::io::copy(
                    &mut response
                        .bytes()
                        .expect("Failed to get response bytes")
                        .as_ref(),
                    &mut dest,
                )
                .expect("Failed to write Tessdata");
                println!("cargo:warning={} downloaded", filename);
            } else {
                println!(
                    "cargo:warning={} already exists, skipping download",
                    filename
                );
            }
        }
    }

    pub(crate) fn warn_about_legacy_native_dirs() {
        let user_data_dir = get_user_data_dir();
        let legacy_dirs: Vec<PathBuf> = ["cache", "third_party", "leptonica", "tesseract"]
            .iter()
            .map(|name| user_data_dir.join(name))
            .filter(|path| path.exists())
            .collect();

        if !legacy_dirs.is_empty() {
            let paths = legacy_dirs
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "cargo:warning=Legacy tesseract-rs native directories are no longer used and may be removed manually: {paths}"
            );
        }
    }

    fn build_and_normalize<F>(name: &str, install_dir: &Path, build_fn: F)
    where
        F: FnOnce(),
    {
        println!("Building {name} library");
        build_fn();
        let possible_lib_names: Vec<String> = if cfg!(target_os = "windows") {
            match name {
                // MSVC debug builds append a "d" suffix to the library name, so
                // list both release and debug variants (see issue #17).
                // Prefer CMake outputs over canonical copies from earlier builds.
                "leptonica" => vec![
                    "libleptonica.lib".to_string(),
                    "leptonica-static.lib".to_string(),
                    "leptonica-1.87.0.lib".to_string(),
                    "leptonica-1.87.0d.lib".to_string(),
                    "leptonicad.lib".to_string(),
                    "leptonica.lib".to_string(),
                ],
                "tesseract" => vec![
                    "libtesseract.lib".to_string(),
                    "tesseract-static.lib".to_string(),
                    "tesseract55.lib".to_string(),
                    "tesseract54.lib".to_string(),
                    "tesseract53.lib".to_string(),
                    "tesseract55d.lib".to_string(),
                    "tesseract54d.lib".to_string(),
                    "tesseract53d.lib".to_string(),
                    "tesseract.lib".to_string(),
                ],
                _ => vec![format!("{}.lib", name)],
            }
        } else {
            vec![format!("lib{}.a", name)]
        };
        let lib_dir = install_dir.join("lib");
        let mut found_lib_path = None;
        for lib_name in &possible_lib_names {
            let lib_path = lib_dir.join(lib_name);
            if lib_path.is_file() {
                found_lib_path = Some(lib_path);
                break;
            }
        }
        let library_path = found_lib_path.unwrap_or_else(|| {
                let files = fs::read_dir(&lib_dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                panic!(
                    "Built {name} library not found in {}. Searched for {possible_lib_names:?}; found {files:?}",
                    lib_dir.display()
                );
        });
        let canonical_name = if cfg!(target_os = "windows") {
            format!("{name}.lib")
        } else {
            format!("lib{name}.a")
        };
        let canonical_path = lib_dir.join(canonical_name);
        // The Rust FFI modules use the stable `tesseract` and `leptonica`
        // link names, while some CMake installs use versioned filenames.
        // Normalize those files inside Cargo's OUT_DIR-backed install tree
        // to match `canonical_name` defined above.
        // For example, copy `tesseract55.lib` to `tesseract.lib` so Rust can
        // link it using the stable name `tesseract`.
        if library_path != canonical_path {
            fs::copy(&library_path, &canonical_path).unwrap_or_else(|error| {
                panic!(
                    "Failed to create canonical {name} library {} from {}: {error}",
                    canonical_path.display(),
                    library_path.display()
                )
            });
        }
        println!("cargo:warning=Built library: {}", library_path.display());
    }
}

#[cfg(feature = "use-system-tesseract")]
mod system_tesseract {
    use std::env;

    /// Link against a system-installed Tesseract (Homebrew, libtesseract-dev,
    /// etc.) instead of compiling the bundled sources.
    ///
    /// Uses pkg-config to locate `tesseract.pc`; leptonica is pulled in
    /// transitively via the `Requires:` entry. libtesseract is C++, so the
    /// C++ standard library runtime is linked explicitly.
    pub fn link_system_tesseract() {
        pkg_config::Config::new()
            .probe("tesseract")
            .expect("use-system-tesseract requires the Tesseract development library (e.g. `brew install tesseract` or `apt install libtesseract-dev`)");

        if cfg!(target_os = "macos") {
            println!("cargo:rustc-link-lib=c++");
        } else if cfg!(target_os = "freebsd") {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=pthread");
        } else if cfg!(target_os = "linux") {
            let uses_clang = env::var("CC")
                .map(|cc| cc.contains("clang"))
                .unwrap_or(false);
            if uses_clang || cfg!(target_env = "musl") {
                println!("cargo:rustc-link-lib=c++");
            } else {
                println!("cargo:rustc-link-lib=stdc++");
            }
            println!("cargo:rustc-link-lib=pthread");
        }
    }
}

fn main() {
    #[cfg(feature = "build-tesseract")]
    build_tesseract::warn_about_legacy_native_dirs();

    #[cfg(feature = "use-system-tesseract")]
    system_tesseract::link_system_tesseract();

    #[cfg(all(feature = "build-tesseract", not(feature = "use-system-tesseract")))]
    build_tesseract::build();

    #[cfg(feature = "embed-tessdata")]
    {
        // In system mode the bundled build (which normally downloads the
        // tessdata files) is skipped, so fetch them here for embedding.
        #[cfg(feature = "use-system-tesseract")]
        build_tesseract::download_tessdata(&build_tesseract::get_user_data_dir());

        generate_embedded_tessdata();
    }
}

#[cfg(feature = "embed-tessdata")]
fn generate_embedded_tessdata() {
    use std::fs;
    use std::path::Path;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let tessdata_dir = build_tesseract::get_user_data_dir().join("tessdata");

    let mut embedded_code = String::new();
    embedded_code.push_str("// Auto-generated embedded tessdata\n");
    embedded_code.push_str("use std::collections::HashMap;\n\n");
    embedded_code.push_str("pub struct EmbeddedTessdata {\n");
    embedded_code.push_str("    data: HashMap<&'static str, &'static [u8]>,\n");
    embedded_code.push_str("}\n\n");
    embedded_code.push_str("impl EmbeddedTessdata {\n");
    embedded_code.push_str("    pub fn new() -> Self {\n");
    embedded_code.push_str("        let mut data = HashMap::new();\n");

    // Embed language files based on environment variables
    let embed_languages =
        std::env::var("TESSERACT_EMBED_LANGUAGES").unwrap_or_else(|_| "eng,tur".to_string());

    let languages: Vec<&str> = embed_languages.split(',').map(|s| s.trim()).collect();

    for lang in &languages {
        let traineddata_file = tessdata_dir.join(format!("{}.traineddata", lang));
        if traineddata_file.exists() {
            embedded_code.push_str(&format!(
                "        data.insert(\"{}\", include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{}.traineddata\")) as &'static [u8]);\n",
                lang, lang
            ));

            // Copy the file to OUT_DIR so include_bytes! can find it
            let dest = Path::new(&out_dir).join(format!("{}.traineddata", lang));
            if let Err(e) = fs::copy(&traineddata_file, &dest) {
                println!("cargo:warning=Failed to copy {}.traineddata: {}", lang, e);
            }
        } else {
            println!(
                "cargo:warning=Language {} not found in tessdata directory",
                lang
            );
        }
    }

    embedded_code.push_str("        Self { data }\n");
    embedded_code.push_str("    }\n\n");
    embedded_code.push_str("    pub fn get(&self, language: &str) -> Option<&'static [u8]> {\n");
    embedded_code.push_str("        self.data.get(language).copied()\n");
    embedded_code.push_str("    }\n\n");
    embedded_code.push_str("    pub fn available_languages(&self) -> Vec<&'static str> {\n");
    embedded_code.push_str("        self.data.keys().copied().collect()\n");
    embedded_code.push_str("    }\n");
    embedded_code.push_str("}\n\n");
    embedded_code.push_str("pub static EMBEDDED_TESSDATA: std::sync::LazyLock<EmbeddedTessdata> = std::sync::LazyLock::new(|| EmbeddedTessdata::new());\n");

    let embedded_file = Path::new(&out_dir).join("embedded_tessdata.rs");
    fs::write(&embedded_file, embedded_code).expect("Failed to write embedded tessdata file");

    println!("cargo:rerun-if-changed={}", tessdata_dir.display());
}

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    // Windows embeds the ICO in resource.lib; rebuild it when icon assets change.
    println!("cargo:rerun-if-changed=icons");
    let config_path = manifest_dir.join("..").join("brand.config.json");
    println!("cargo:rerun-if-changed={}", config_path.display());

    let raw = fs::read_to_string(&config_path).expect("failed to read brand.config.json");
    let config: serde_json::Value =
        serde_json::from_str(&raw).expect("failed to parse brand.config.json");

    let package_name = config["packageName"]
        .as_str()
        .expect("brand.config.json field packageName must be a string");
    assert_eq!(
        env::var("CARGO_PKG_NAME").expect("missing Cargo package name"),
        package_name,
        "Cargo package name is out of sync; run npm run brand:sync"
    );

    let tauri_config_path = manifest_dir.join("tauri.conf.json");
    println!("cargo:rerun-if-changed={}", tauri_config_path.display());
    let tauri_config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&tauri_config_path).expect("failed to read tauri.conf.json"),
    )
    .expect("failed to parse tauri.conf.json");
    assert_eq!(
        tauri_config["productName"].as_str(),
        config["displayName"].as_str(),
        "Tauri product name is out of sync; run npm run brand:sync"
    );
    assert_eq!(
        tauri_config["identifier"].as_str(),
        config["bundleIdentifier"].as_str(),
        "Tauri identifier is out of sync; run npm run brand:sync"
    );

    for (field, env_name) in [
        ("displayName", "APP_DISPLAY_NAME"),
        ("bundleIdentifier", "APP_BUNDLE_IDENTIFIER"),
        ("storagePrefix", "APP_STORAGE_PREFIX"),
        ("projectDataDirectory", "APP_PROJECT_DATA_DIRECTORY"),
        ("startupRegistryName", "APP_STARTUP_REGISTRY_NAME"),
        ("pluginNamespace", "APP_PLUGIN_NAMESPACE"),
    ] {
        let value = config[field]
            .as_str()
            .unwrap_or_else(|| panic!("brand.config.json field {field} must be a string"));
        println!("cargo:rustc-env={env_name}={value}");
    }

    // Apply the same Windows activation context to desktop, CLI and unit tests.
    // Keep it out of resource.lib to avoid a duplicate MANIFEST resource in bins.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let windows_manifest = manifest_dir.join("windows-app.manifest");
        println!("cargo:rerun-if-changed={}", windows_manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", windows_manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new_without_app_manifest(),
    )).expect("failed to build Tauri resources")
}

pub const DISPLAY_NAME: &str = env!("APP_DISPLAY_NAME");
pub const IDENTIFIER: &str = env!("APP_BUNDLE_IDENTIFIER");
#[cfg(test)]
pub const STORAGE_PREFIX: &str = env!("APP_STORAGE_PREFIX");
pub const PROJECT_DATA_DIRECTORY: &str = env!("APP_PROJECT_DATA_DIRECTORY");
pub const STARTUP_REGISTRY_NAME: &str = env!("APP_STARTUP_REGISTRY_NAME");
pub const PLUGIN_NAMESPACE: &str = env!("APP_PLUGIN_NAMESPACE");
pub const MAIN_DATABASE_NAME: &str = concat!(env!("APP_STORAGE_PREFIX"), ".sqlite3");
pub const RUNTIME_DATABASE_NAME: &str = concat!(env!("APP_STORAGE_PREFIX"), "-runtime.sqlite3");
pub const OBSERVABILITY_DATABASE_NAME: &str =
    concat!(env!("APP_STORAGE_PREFIX"), "-observability.sqlite3");
pub const IMAGE_UPLOADS_DIRECTORY: &str =
    concat!(env!("APP_PROJECT_DATA_DIRECTORY"), "/uploads/images");
pub const TRAY_ID: &str = concat!(env!("APP_STORAGE_PREFIX"), "-tray");

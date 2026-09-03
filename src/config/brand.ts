import brandConfig from "../../brand.config.json";

export const APP_NAME = brandConfig.displayName;
export const APP_PACKAGE_NAME = brandConfig.packageName;
export const APP_IDENTIFIER = brandConfig.bundleIdentifier;
export const APP_STORAGE_PREFIX = brandConfig.storagePrefix;
export const PROJECT_DATA_DIRECTORY = brandConfig.projectDataDirectory;
export const APP_PLUGIN_NAMESPACE = brandConfig.pluginNamespace;
export const LEGACY_APP_NAME = brandConfig.legacyDisplayName;

export default brandConfig;

import { describe, expect, it } from "vitest";
import brandConfig, {
  APP_IDENTIFIER,
  APP_NAME,
  APP_PACKAGE_NAME,
  APP_STORAGE_PREFIX,
  PROJECT_DATA_DIRECTORY
} from "./brand";

describe("brand configuration", () => {
  it("uses NanoDesk as the current product name", () => {
    expect(APP_NAME).toBe("NanoDesk");
    expect(APP_PACKAGE_NAME).toBe("nano-desk");
    expect(brandConfig.desktopBinaryName).toBe("nano-desk");
  });

  it("uses NanoDesk identifiers for new persisted data", () => {
    expect(APP_IDENTIFIER).toBe("com.nanodesk.desktop");
    expect(APP_STORAGE_PREFIX).toBe("nanodesk");
    expect(PROJECT_DATA_DIRECTORY).toBe(".nanodesk");
  });
});

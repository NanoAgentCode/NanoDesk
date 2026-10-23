export interface GithubRelease {
  tag_name: string;
  body?: string;
  draft?: boolean;
  prerelease?: boolean;
  assets: { name: string; browser_download_url: string }[];
}

export interface AppUpdate { version: string; notes: string; downloadUrl: string }

function versionParts(value: string): number[] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:\+[\w.-]+)?$/.exec(value);
  return match ? match.slice(1).map(Number) : null;
}

export function selectAppUpdate(current: string, release: GithubRelease | null, arch: string): AppUpdate | null {
  if (!release || release.draft || release.prerelease) return null;
  const local = versionParts(current);
  const remote = versionParts(release.tag_name);
  if (!local || !remote) return null;
  const difference = remote.findIndex((value, index) => value !== local[index]);
  if (difference < 0 || remote[difference] < local[difference]) return null;
  const asset = release.assets.find(item =>
    item.name.toLowerCase().endsWith(`_${arch}-setup.exe`) &&
    item.browser_download_url.startsWith("https://github.com/NanoAgentCode/NanoDesk/releases/download/")
  );
  // A release without a compatible installer must not offer a broken download.
  if (!asset) return null;
  return { version: release.tag_name, notes: release.body || "此版本未提供更新说明。", downloadUrl: asset.browser_download_url };
}

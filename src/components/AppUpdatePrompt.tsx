import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Group, Modal, Stack, Text } from "@mantine/core";
import { openExternalUrl } from "../api/desktop";
import { selectAppUpdate, type AppUpdate, type GithubRelease } from "../lib/appUpdate";

// One checker in the main window; auxiliary windows must not duplicate prompts.
export default function AppUpdatePrompt() {
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true;
    let checking = false;
    const prompted = new Set<string>();
    const check = async () => {
      if (checking) return;
      checking = true;
      try {
        const result = await invoke<{ currentVersion: string; arch: string; release: GithubRelease | null }>("check_app_update");
        const next = selectAppUpdate(result.currentVersion, result.release, result.arch);
        if (active && next && !prompted.has(next.version)) {
          prompted.add(next.version);
          setError("");
          setUpdate(next);
        }
      } catch (err) {
        console.debug("Automatic update check failed; will retry later", err);
      } finally { checking = false; }
    };
    const startup = window.setTimeout(() => { void check(); }, 5000);
    const interval = window.setInterval(() => { void check(); }, 6 * 60 * 60 * 1000);
    return () => { active = false; window.clearTimeout(startup); window.clearInterval(interval); };
  }, []);
  const download = async () => {
    if (!update) return;
    setBusy(true);
    setError("");
    try { await openExternalUrl(update.downloadUrl); }
    catch { setError("无法打开下载链接，请重试。"); }
    finally { setBusy(false); }
  };
  return <Modal opened={update !== null} onClose={() => setUpdate(null)} title="发现新版本" centered>
    <Stack>
      <Text fw={600}>NanoDesk {update?.version}</Text>
      <Text size="sm" style={{ whiteSpace: "pre-wrap", maxHeight: 260, overflowY: "auto" }}>{update?.notes}</Text>
      <Text size="sm" c="dimmed">通过系统浏览器从 GitHub 下载安装包，下载完成后运行安装。</Text>
      {error && <Alert color="red">{error}</Alert>}
      <Group justify="flex-end">
        <Button variant="default" onClick={() => setUpdate(null)}>稍后再说</Button>
        <Button loading={busy} onClick={() => { void download(); }}>从 GitHub 下载</Button>
      </Group>
    </Stack>
  </Modal>;
}

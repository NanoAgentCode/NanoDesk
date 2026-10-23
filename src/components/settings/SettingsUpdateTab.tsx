import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Alert, Button, Divider, Group, Stack, Text } from "@mantine/core";
import { FileText, RefreshCw } from "lucide-react";
import { checkAppUpdate, RELEASE_HISTORY_URL, showAppUpdate } from "../../api/updates";
import { openExternalUrl } from "../../api/desktop";
import SettingsSectionHeader from "./SettingsSectionHeader";
import "./SettingsUpdateTab.css";

export default function SettingsUpdateTab() {
  const [version, setVersion] = useState("");
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const active = useRef(false);
  const pending = useRef(false);
  useEffect(() => {
    active.current = true;
    void getVersion().then(value => { if (active.current) setVersion(value); })
      .catch(() => { if (active.current) setVersion("读取失败"); });
    return () => { active.current = false; };
  }, []);
  const check = async () => {
    if (pending.current) return;
    pending.current = true;
    setChecking(true); setError(""); setMessage("");
    try {
      const update = await checkAppUpdate();
      if (!active.current) return;
      if (update) { showAppUpdate(update); setMessage(`发现新版本 ${update.version}，可在提示窗口下载。`); }
      else setMessage("未发现可用更新。");
    } catch {
      if (active.current) setError("检查更新失败，请检查网络或稍后重试。GitHub 可能暂时限制了请求频率。");
    } finally {
      pending.current = false;
      if (active.current) setChecking(false);
    }
  };
  const openHistory = async () => {
    setError("");
    try { await openExternalUrl(RELEASE_HISTORY_URL); }
    catch { if (active.current) setError("无法打开更新日志，请重试。"); }
  };
  return <div className="settings-tab-content update-tab-content">
    <SettingsSectionHeader title="更新应用" />
    <Text size="sm" c="dimmed">查看当前版本、更新日志并检查应用更新</Text>
    <Stack className="update-settings-card" gap="lg">
      <Stack gap={6}>
        <Text fw={600}>当前版本</Text>
        <Text size="sm" c="dimmed">自动检查新版本，也可随时手动检查。</Text>
        <Text className="update-version" aria-label="当前版本">{version || "读取中…"}</Text>
      </Stack>
      <Divider />
      <Group justify="space-between" wrap="nowrap" gap="lg">
        <Stack gap={4}><Text fw={600}>更新日志</Text><Text size="sm" c="dimmed">查看 GitHub 上的历史版本与更新说明</Text></Stack>
        <Button variant="default" leftSection={<FileText size={16} />} onClick={() => { void openHistory(); }}>更新日志</Button>
      </Group>
      <Divider />
      <Group justify="space-between" wrap="nowrap" gap="lg">
        <Stack gap={4}><Text fw={600}>检查更新</Text><Text size="sm" c="dimmed">发现新版本后提示下载，从 GitHub 获取安装包。</Text></Stack>
        <Button loading={checking} disabled={checking} leftSection={<RefreshCw size={16} />} onClick={() => { void check(); }}>{checking ? "正在检查…" : "检查更新"}</Button>
      </Group>
      {message && <Text size="sm" role="status">{message}</Text>}
      {error && <Alert color="red" role="alert">{error}</Alert>}
    </Stack>
  </div>;
}

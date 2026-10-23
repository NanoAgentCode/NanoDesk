import { useEffect, useRef, useState } from "react";
import { Alert, Button, Group, Modal, Progress, Stack, Text } from "@mantine/core";
import { type AppUpdate } from "../lib/appUpdate";
import { checkAppUpdate, subscribeAppUpdate, downloadAppUpdate, installAppUpdate, type DownloadProgress } from "../api/updates";

// One checker in the main window; auxiliary windows must not duplicate prompts.
export default function AppUpdatePrompt() {
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const pending = useRef(false);
  const currentVersion = useRef("");
  useEffect(() => {
    let active = true;
    let checking = false;
    const prompted = new Set<string>();
    const unsubscribe = subscribeAppUpdate(next => {
      if (pending.current) return;
      if (currentVersion.current !== next.version) { setReady(false); setProgress(null); }
      currentVersion.current = next.version;
      prompted.add(next.version);
      setError("");
      setUpdate(next);
    });
    const check = async () => {
      if (checking) return;
      checking = true;
      try {
        const next = await checkAppUpdate();
        if (active && next && !pending.current && !prompted.has(next.version)) {
          if (currentVersion.current !== next.version) { setReady(false); setProgress(null); }
          currentVersion.current = next.version;
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
    return () => { active = false; unsubscribe(); window.clearTimeout(startup); window.clearInterval(interval); };
  }, []);
  const download = async () => {
    if (!update || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setProgress(null);
    try { await downloadAppUpdate(update.downloadUrl, setProgress); setReady(true); }
    catch { setError("下载失败，请检查网络后重试。"); }
    finally { pending.current = false; setBusy(false); }
  };
  const install = async () => {
    if (pending.current || !ready) return;
    pending.current = true; setBusy(true); setError("");
    try { await installAppUpdate(); }
    catch { setError("无法启动安装程序，请重试。"); }
    finally { pending.current = false; setBusy(false); }
  };
  const close = () => { if (!pending.current) setUpdate(null); };
  return <Modal opened={update !== null} onClose={close} closeOnClickOutside={!busy} closeOnEscape={!busy} withCloseButton={!busy} title="发现新版本" centered>
    <Stack>
      <Text fw={600}>NanoDesk {update?.version}</Text>
      <Text size="sm" style={{ whiteSpace: "pre-wrap", maxHeight: 260, overflowY: "auto" }}>{update?.notes}</Text>
      <Text size="sm" c="dimmed">{ready ? "下载完成。点击立即安装将退出 NanoDesk 并启动安装程序，请先保存工作。" : "在应用内下载安装包，完成后由你选择立即安装。"}</Text>
      {!ready && progress && <Stack gap={4}>
        <Progress aria-label="更新下载进度" value={progress.total ? Math.min(100, progress.downloaded / progress.total * 100) : 0} animated={busy} />
        <Text size="sm" role="status">已下载 {(progress.downloaded / 1048576).toFixed(1)} MB{progress.total ? ` / ${(progress.total / 1048576).toFixed(1)} MB` : ""}</Text>
      </Stack>}
      {error && <Alert color="red">{error}</Alert>}
      <Group justify="flex-end">
        <Button variant="default" disabled={busy} onClick={close}>稍后再说</Button>
        <Button loading={busy} onClick={() => { void (ready ? install() : download()); }}>{ready ? "立即安装" : "下载更新"}</Button>
      </Group>
    </Stack>
  </Modal>;
}

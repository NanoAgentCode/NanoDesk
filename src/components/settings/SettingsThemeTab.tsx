import { useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { Checkbox, Group, Radio, SegmentedControl, Stack, Switch, Text } from "@mantine/core";
import TaskSettingsSection from "./TaskSettingsSection";
import { themeLabels } from "../../lib/appHelpers";
import type { ThemeMode } from "../../types";
import { getAutostart, setAutostart } from "../../api";
import {
  getStoredCloseAction,
  getStoredCloseSkipPrompt,
  setStoredCloseAction,
  setStoredCloseSkipPrompt,
  subscribeClosePreferencesChanged,
  type CloseAction
} from "../../lib/closeBehavior";

interface SettingsThemeTabProps {
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
}

export default function SettingsThemeTab({ themeMode, setThemeMode }: SettingsThemeTabProps) {
  const [autostart, setAutostartState] = useState(false);
  const [autostartBusy, setAutostartBusy] = useState(false);
  const [autostartLoaded, setAutostartLoaded] = useState(false);
  const [closeAction, setCloseAction] = useState<CloseAction>(() => getStoredCloseAction());
  const [closeSkipPrompt, setCloseSkipPrompt] = useState(() => getStoredCloseSkipPrompt());

  useEffect(() => {
    let active = true;
    getAutostart()
      .then((enabled) => {
        if (active) {
          setAutostartState(enabled);
        }
      })
      .catch((err) => console.error("Failed to query autostart status:", err))
      .finally(() => {
        if (active) {
          setAutostartLoaded(true);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    return subscribeClosePreferencesChanged((preferences) => {
      setCloseAction(preferences.action);
      setCloseSkipPrompt(preferences.skipPrompt);
    });
  }, []);

  const handleAutostartChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const checked = event.target.checked;
    setAutostartBusy(true);
    setAutostartState(checked);
    try {
      await setAutostart(checked);
    } catch (err) {
      console.error("Failed to update autostart status:", err);
      setAutostartState(!checked);
    } finally {
      setAutostartBusy(false);
    }
  };

  const handleCloseActionChange = (action: CloseAction) => {
    setCloseAction(action);
    setStoredCloseAction(action);
  };

  const handleCloseSkipPromptChange = (checked: boolean) => {
    setCloseSkipPrompt(checked);
    setStoredCloseSkipPrompt(checked);
  };

  return (
    <div className="settings-tab-content theme-tab-content">
      <h3>通用设置</h3>
      <p className="description">自定义系统主题与基础启动配置。</p>

      <Text component="h4" fw={650} size="sm" mt="lg" mb="xs">界面主题</Text>
      <SegmentedControl
        className="settings-theme-control"
        fullWidth
        mb="xl"
        value={themeMode}
        onChange={(value) => setThemeMode(value as ThemeMode)}
        data={([
          ["system", Monitor],
          ["light", Sun],
          ["dark", Moon]
        ] as const).map(([mode, Icon]) => ({
          value: mode,
          label: <Group gap={6} justify="center"><Icon size={15} /><span>{themeLabels[mode]}</span></Group>
        }))}
        aria-label="主题切换"
      />

      <Text component="h4" fw={650} size="sm" mt="lg" mb="sm">系统选项</Text>
      <Stack gap="md">
        <Radio.Group
          value={closeAction}
          onChange={(value) => handleCloseActionChange(value as CloseAction)}
          label="关闭按钮行为"
        >
          <Stack gap="xs" mt="xs">
            <Radio value="tray" label="最小化到系统托盘" />
            <Radio value="quit" label="退出应用" />
          </Stack>
        </Radio.Group>
        <Checkbox
          checked={closeSkipPrompt}
          onChange={(event) => handleCloseSkipPromptChange(event.currentTarget.checked)}
          label="关闭时不再提示"
        />
        <Switch
          checked={autostart}
          onChange={handleAutostartChange}
          disabled={!autostartLoaded || autostartBusy}
          label="开机自启动"
        />
      </Stack>
      <TaskSettingsSection />
    </div>
  );
}

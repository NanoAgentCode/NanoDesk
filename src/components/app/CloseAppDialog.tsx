import { Button, Checkbox, Group, Modal, Radio, Stack, Text, ThemeIcon } from "@mantine/core";
import { Power } from "lucide-react";
import type { useAppCloseBehavior } from "../../hooks/useAppCloseBehavior";
import type { CloseAction } from "../../lib/closeBehavior";
export default function CloseAppDialog({
  closePromptOpen,
  closeAction,
  setCloseAction,
  closeDontAsk,
  setCloseDontAsk,
  handleCancelClosePrompt,
  handleConfirmClosePrompt
}: ReturnType<typeof useAppCloseBehavior>) {
  return (
    <>
      {closePromptOpen && (
        <Modal
          opened
          onClose={handleCancelClosePrompt}
          size="sm"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="orange" size="md">
                <Power size={18} />
              </ThemeIcon>
              <Text fw={650}>点击关闭按钮</Text>
            </Group>
          }
        >
          <Stack gap="lg">
            <Radio.Group
              value={closeAction}
              onChange={(value) => setCloseAction(value as CloseAction)}
              label="关闭按钮行为"
            >
              <Stack gap="xs" mt="xs">
                <Radio value="tray" label="最小化到系统托盘" />
                <Radio value="quit" label="退出应用" />
              </Stack>
            </Radio.Group>
            <Checkbox
              checked={closeDontAsk}
              onChange={(event) => setCloseDontAsk(event.currentTarget.checked)}
              label="不再提示"
            />
            <Group justify="flex-end">
              <Button variant="default" onClick={handleCancelClosePrompt}>
                取消
              </Button>
              <Button leftSection={<Power size={15} />} onClick={handleConfirmClosePrompt}>
                确定
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}
    </>
  );
}

import { Button, Group, Modal, Stack, Text, TextInput, ThemeIcon } from "@mantine/core";
import { Edit3 } from "lucide-react";
import type { Conversation } from "../../types";
interface Props {
  renameTarget: Conversation | null;
  renameTitle: string;
  setRenameTitle: (title: string) => void;
  closeRenameDialog: () => void;
  handleConfirmRename: () => Promise<void>;
}
export default function RenameConversationDialog({
  renameTarget,
  renameTitle,
  setRenameTitle,
  closeRenameDialog,
  handleConfirmRename
}: Props) {
  return (
    <>
      {renameTarget && (
        <Modal
          opened
          onClose={closeRenameDialog}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" size="md">
                <Edit3 size={18} />
              </ThemeIcon>
              <Text fw={650}>重命名会话</Text>
            </Group>
          }
        >
          <Stack gap="lg">
            <TextInput
              label="会话名称"
              value={renameTitle}
              onChange={(event) => setRenameTitle(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void handleConfirmRename();
                }
              }}
              autoFocus
            />
            <Group justify="flex-end">
              <Button variant="default" onClick={closeRenameDialog}>
                取消
              </Button>
              <Button leftSection={<Edit3 size={15} />} onClick={() => void handleConfirmRename()}>
                保存修改
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}
    </>
  );
}
